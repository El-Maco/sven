use core::net::Ipv4Addr;

use embassy_net::{Config, Ipv4Cidr, Runner, StaticConfigV4};
use esp_wifi::wifi::{
    ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice,
    WifiState,
};
use log::{debug, error, info};

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

/// The desk address. Override at build time with `STATIC_IP=...`.
pub const STATIC_IP: &str = match option_env!("STATIC_IP") {
    Some(value) => value,
    None => "192.168.1.10",
};

/// The default gateway. Override at build time with `GATEWAY=...`.
pub const GATEWAY: &str = match option_env!("GATEWAY") {
    Some(value) => value,
    None => "192.168.1.1",
};

fn prefix_len() -> u8 {
    match option_env!("PREFIX") {
        Some(value) => value.parse().unwrap_or(24),
        None => 24,
    }
}

fn parse_octet(part: Option<&str>) -> u8 {
    part.and_then(|value| value.parse().ok()).unwrap_or(0)
}

pub fn parse_ip(value: &str) -> Ipv4Addr {
    let mut parts = value.split('.');
    Ipv4Addr::new(
        parse_octet(parts.next()),
        parse_octet(parts.next()),
        parse_octet(parts.next()),
        parse_octet(parts.next()),
    )
}

pub fn net_config() -> Config {
    Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(parse_ip(STATIC_IP), prefix_len()),
        gateway: Some(parse_ip(GATEWAY)),
        dns_servers: heapless::Vec::new(),
    })
}

#[embassy_executor::task]
pub async fn connection_task(mut controller: WifiController<'static>) {
    info!("WiFi connection task started");
    debug!("Device capabilities: {:?}", controller.capabilities());

    loop {
        if let WifiState::StaConnected = esp_wifi::wifi::wifi_state() {
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            sleep(5000).await;
        }

        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: SSID.try_into().unwrap(),
                password: PASSWORD.try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            info!("Starting WiFi");
            controller.start_async().await.unwrap();
            info!("WiFi started");
        }

        info!("Connecting to WiFi...");
        match controller.connect_async().await {
            Ok(_) => info!("WiFi connected"),
            Err(error) => {
                error!("Failed to connect to WiFi: {error:?}");
                sleep(5000).await;
            }
        }
    }
}

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, WifiDevice<'static, WifiStaDevice>>) -> ! {
    runner.run().await
}

pub async fn sleep(millis: u32) {
    embassy_time::Timer::after(embassy_time::Duration::from_millis(millis as u64)).await;
}
