//! Retained MQTT persistence for the desk estimate and the position presets.
//!
//! This is a deliberately small MQTT 3.1.1 client: connect, subscribe for the
//! initial retained read, unsubscribe, then publish state with the retain flag.

use core::str::FromStr;

use embassy_futures::select::{select3, Either3};
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{with_timeout, Duration, Instant, Timer};
use embedded_io_async::Write;
use heapless::Vec;
use log::{info, warn};

use crate::desk::state::{
    ensure_seeded_positions, positions, replace_positions, sanitize_positions, DeskState,
    Positions, MAX_HEIGHT_MM, MIN_HEIGHT_MM, POSITIONS_CHANGED, STATE,
};
use crate::net::parse_ip;

pub const STATE_TOPIC: &str = "sven/state";
pub const POSITIONS_TOPIC: &str = "sven/positions";

const CLIENT_ID: &str = "sven-esp32";
const HEARTBEAT: Duration = Duration::from_secs(30);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

type Packet = Vec<u8, 1024>;

pub struct BootData {
    pub state: Option<DeskState>,
}

pub static BOOT: Channel<CriticalSectionRawMutex, BootData, 1> = Channel::new();

pub fn enabled() -> bool {
    mqtt_host().is_some()
}

fn mqtt_host() -> Option<&'static str> {
    option_env!("MQTT_HOST").filter(|host| !host.is_empty())
}

fn mqtt_port() -> u16 {
    option_env!("MQTT_PORT")
        .and_then(|value| u16::from_str(value).ok())
        .unwrap_or(1883)
}

fn username() -> Option<&'static str> {
    option_env!("MQTT_USERNAME").filter(|value| !value.is_empty())
}

fn password() -> Option<&'static str> {
    option_env!("MQTT_PASSWORD").filter(|value| !value.is_empty())
}

fn put_u16(buffer: &mut Packet, value: u16) -> Result<(), ()> {
    buffer
        .extend_from_slice(&value.to_be_bytes())
        .map_err(|_| ())
}

fn put_str(buffer: &mut Packet, value: &str) -> Result<(), ()> {
    put_u16(buffer, value.len() as u16)?;
    buffer.extend_from_slice(value.as_bytes()).map_err(|_| ())
}

fn put_remaining_length(buffer: &mut Packet, mut length: usize) {
    loop {
        let mut byte = (length % 128) as u8;
        length /= 128;
        if length > 0 {
            byte |= 0x80;
        }
        buffer.push(byte).ok();
        if length == 0 {
            break;
        }
    }
}

fn packet(header: u8, body: &[u8]) -> Result<Packet, ()> {
    let mut buffer = Packet::new();
    buffer.push(header).map_err(|_| ())?;
    put_remaining_length(&mut buffer, body.len());
    buffer.extend_from_slice(body).map_err(|_| ())?;
    Ok(buffer)
}

fn connect_packet() -> Result<Packet, ()> {
    let mut body = Packet::new();
    put_str(&mut body, "MQTT")?;
    body.push(4).map_err(|_| ())?;

    let mut flags = 0x02; // Clean session.
    if username().is_some() {
        flags |= 0x80;
        if password().is_some() {
            flags |= 0x40;
        }
    }
    body.push(flags).map_err(|_| ())?;
    put_u16(&mut body, 60)?;
    put_str(&mut body, CLIENT_ID)?;
    if let Some(username) = username() {
        put_str(&mut body, username)?;
        if let Some(password) = password() {
            put_str(&mut body, password)?;
        }
    }

    packet(0x10, &body)
}

fn subscribe_packet() -> Result<Packet, ()> {
    let mut body = Packet::new();
    put_u16(&mut body, 1)?;
    for topic in [STATE_TOPIC, POSITIONS_TOPIC] {
        put_str(&mut body, topic)?;
        body.push(0).map_err(|_| ())?; // QoS 0.
    }
    packet(0x82, &body)
}

fn unsubscribe_packet() -> Result<Packet, ()> {
    let mut body = Packet::new();
    put_u16(&mut body, 2)?;
    for topic in [STATE_TOPIC, POSITIONS_TOPIC] {
        put_str(&mut body, topic)?;
    }
    packet(0xA2, &body)
}

fn publish_packet(topic: &str, payload: &[u8]) -> Result<Packet, ()> {
    let mut body = Packet::new();
    put_str(&mut body, topic)?;
    body.extend_from_slice(payload).map_err(|_| ())?;
    packet(0x31, &body) // QoS 0, retain.
}

async fn send(socket: &mut TcpSocket<'_>, packet: &[u8]) -> Result<(), ()> {
    socket.write_all(packet).await.map_err(|_| ())
}

async fn read_exact(socket: &mut TcpSocket<'_>, buffer: &mut [u8]) -> Result<(), ()> {
    let mut offset = 0;
    while offset < buffer.len() {
        let read = socket.read(&mut buffer[offset..]).await.map_err(|_| ())?;
        if read == 0 {
            return Err(());
        }
        offset += read;
    }
    Ok(())
}

async fn read_packet(socket: &mut TcpSocket<'_>) -> Result<(u8, Packet), ()> {
    let mut header = [0u8; 1];
    read_exact(socket, &mut header).await?;

    let mut length = 0usize;
    let mut multiplier = 1usize;
    for _ in 0..4 {
        let mut byte = [0u8; 1];
        read_exact(socket, &mut byte).await?;
        length += ((byte[0] & 0x7f) as usize) * multiplier;
        if byte[0] & 0x80 == 0 {
            break;
        }
        multiplier *= 128;
    }
    if length > 1024 {
        return Err(());
    }

    let mut body = Packet::new();
    body.resize(length, 0).map_err(|_| ())?;
    read_exact(socket, &mut body).await?;
    Ok((header[0], body))
}

fn parse_publish(header: u8, body: &[u8]) -> Option<(&str, &[u8])> {
    if header >> 4 != 3 || body.len() < 2 {
        return None;
    }

    let topic_length = u16::from_be_bytes([body[0], body[1]]) as usize;
    if body.len() < 2 + topic_length {
        return None;
    }
    let topic = core::str::from_utf8(&body[2..2 + topic_length]).ok()?;
    let mut payload = &body[2 + topic_length..];

    // QoS 0 is requested, but tolerate a packet id if a broker sends a higher QoS.
    if (header >> 1) & 0x03 != 0 {
        if payload.len() < 2 {
            return None;
        }
        payload = &payload[2..];
    }

    Some((topic, payload))
}

async fn publish_state(socket: &mut TcpSocket<'_>, state: &DeskState) -> Result<(), ()> {
    let Ok(json): Result<heapless::String<256>, _> = serde_json_core::to_string(state) else {
        return Ok(());
    };
    send(socket, &publish_packet(STATE_TOPIC, json.as_bytes())?).await
}

async fn publish_positions(socket: &mut TcpSocket<'_>) -> Result<(), ()> {
    let Ok(json): Result<heapless::String<768>, _> = serde_json_core::to_string(&positions())
    else {
        return Ok(());
    };
    send(socket, &publish_packet(POSITIONS_TOPIC, json.as_bytes())?).await
}

async fn run_session(
    socket: &mut TcpSocket<'_>,
    boot_deadline: Instant,
    boot_done: &mut bool,
) -> Result<(), ()> {
    send(socket, &connect_packet()?).await?;
    let (header, body) = with_timeout(SOCKET_TIMEOUT, read_packet(socket))
        .await
        .map_err(|_| ())??;
    if header >> 4 != 2 || body.len() < 2 || body[1] != 0 {
        return Err(());
    }
    info!("MQTT connected to broker");

    let mut boot_state = None;
    if !*boot_done && Instant::now() <= boot_deadline {
        send(socket, &subscribe_packet()?).await?;

        let mut got_state = false;
        let mut got_positions = false;
        loop {
            if got_state && got_positions {
                break;
            }
            let Some(remaining) = boot_deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let Ok(read) = with_timeout(remaining, read_packet(socket)).await else {
                break;
            };
            let (header, body) = read?;
            let Some((topic, payload)) = parse_publish(header, &body) else {
                continue;
            };

            match topic {
                STATE_TOPIC => {
                    if let Ok((state, _)) = serde_json_core::from_slice::<DeskState>(payload) {
                        if state.calibrated
                            && (MIN_HEIGHT_MM..=MAX_HEIGHT_MM).contains(&state.height_mm)
                        {
                            boot_state = Some(state);
                        }
                        got_state = true;
                    }
                }
                POSITIONS_TOPIC => {
                    if let Ok((list, _)) = serde_json_core::from_slice::<Positions>(payload) {
                        replace_positions(sanitize_positions(list));
                        got_positions = true;
                    }
                }
                _ => {}
            }
        }

        let _ = send(socket, &unsubscribe_packet()?).await;
        *boot_done = true;
        BOOT.try_send(BootData {
            state: boot_state.clone(),
        })
        .ok();
    }

    if !*boot_done {
        *boot_done = true;
    }
    ensure_seeded_positions();

    let mut state_receiver = STATE
        .receiver()
        .expect("STATE watch receiver slots exhausted");
    let current = state_receiver.try_get().unwrap_or(DeskState::UNKNOWN);
    let initial = boot_state.unwrap_or(current);

    publish_state(socket, &initial).await?;
    publish_positions(socket).await?;

    info!("MQTT persistence ready");

    loop {
        match select3(
            state_receiver.changed(),
            POSITIONS_CHANGED.wait(),
            Timer::after(HEARTBEAT),
        )
        .await
        {
            Either3::First(state) => publish_state(socket, &state).await?,
            Either3::Second(()) => publish_positions(socket).await?,
            Either3::Third(()) => {
                let state = state_receiver.try_get().unwrap_or(DeskState::UNKNOWN);
                publish_state(socket, &state).await?;
                publish_positions(socket).await?;
            }
        }
    }
}

#[embassy_executor::task]
pub async fn mqtt_task(stack: Stack<'static>) -> ! {
    let Some(host) = mqtt_host() else {
        loop {
            Timer::after(Duration::from_secs(3600)).await;
        }
    };

    let host = parse_ip(host);
    let port = mqtt_port();
    let boot_deadline = Instant::now() + Duration::from_secs(5);
    let mut boot_done = false;
    info!("MQTT task started, broker {}:{}", host, port);

    loop {
        let mut rx = [0u8; 1024];
        let mut tx = [0u8; 1024];
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(SOCKET_TIMEOUT));

        match with_timeout(CONNECT_TIMEOUT, socket.connect((host, port))).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                warn!("MQTT TCP connect failed: {:?}", error);
                Timer::after(Duration::from_secs(2)).await;
                continue;
            }
            Err(_) => {
                warn!("MQTT TCP connect timed out");
                Timer::after(Duration::from_secs(2)).await;
                continue;
            }
        }

        if let Err(()) = run_session(&mut socket, boot_deadline, &mut boot_done).await {
            warn!("MQTT session ended");
        }

        Timer::after(Duration::from_secs(2)).await;
    }
}
