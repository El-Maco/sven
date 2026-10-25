#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_time::Duration;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, Level, Output};
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::wifi::WifiStaDevice;
use esp_wifi::EspWifiController;
use log::info;
use picoserve::AppBuilder;

use sven_esp32::desk::desk_task;
use sven_esp32::http::{web_task, AppProps, WEB_TASK_POOL_SIZE};
use sven_esp32::net::{connection_task, net_config, net_task};

extern crate alloc;

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(80 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);

    esp_println::logger::init_logger_from_env();

    let init = &*mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timg0.timer0, rng.clone(), peripherals.RADIO_CLK).unwrap()
    );

    let (wifi_device, wifi_controller) =
        esp_wifi::wifi::new_with_mode(&init, peripherals.WIFI, WifiStaDevice).unwrap();

    esp_hal_embassy::init(timg0.timer1);
    info!("Embassy initialized");

    let seed = (rng.random() as u64) << 32 | rng.random() as u64;
    let (stack, runner) = embassy_net::new(
        wifi_device,
        net_config(),
        mk_static!(StackResources<8>, StackResources::new()),
        seed,
    );

    spawner.spawn(connection_task(wifi_controller)).ok();
    spawner.spawn(net_task(runner)).ok();

    info!("Waiting for network...");
    stack.wait_config_up().await;

    if let Some(config) = stack.config_v4() {
        info!("Sven is reachable at http://{}", config.address);
    } else {
        info!("Sven is reachable at http://{}", sven_esp32::net::STATIC_IP);
    }

    let pin_up = Output::new(peripherals.GPIO5, Level::Low);
    let pin_down = Output::new(peripherals.GPIO7, Level::Low);
    let button_up = Input::new(peripherals.GPIO9, esp_hal::gpio::Pull::Down);
    let button_down = Input::new(peripherals.GPIO10, esp_hal::gpio::Pull::Down);

    spawner
        .spawn(desk_task(pin_up, pin_down, button_up, button_down))
        .ok();

    let app = picoserve::make_static!(picoserve::AppRouter<AppProps>, AppProps.build_app());

    let http_config = picoserve::make_static!(
        picoserve::Config<Duration>,
        picoserve::Config::new(picoserve::Timeouts {
            start_read_request: Some(Duration::from_secs(5)),
            persistent_start_read_request: Some(Duration::from_secs(1)),
            read_request: Some(Duration::from_secs(1)),
            write: Some(Duration::from_secs(5)),
        })
        .keep_connection_alive()
    );

    for task_id in 0..WEB_TASK_POOL_SIZE {
        spawner
            .spawn(web_task(task_id, stack, app, http_config))
            .ok();
    }

    loop {
        embassy_time::Timer::after(Duration::from_secs(3600)).await;
    }
}
