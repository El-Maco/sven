pub mod gpio;
pub mod state;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Duration;
use esp_hal::gpio::{Input, Output};
use log::info;

use gpio::PulsePin;
use state::{DeskCommand, SvenState};

/// Commands from HTTP handlers to the single task that owns the motor pins.
pub static COMMANDS: Channel<CriticalSectionRawMutex, DeskCommand, 8> = Channel::new();

/// Sole owner of `SvenState`. HTTP handlers must never touch the pins directly.
/// The task alternates between queued commands and polling the physical
/// buttons, so both control paths share one motion model.
#[embassy_executor::task]
pub async fn desk_task(
    pin_up: Output<'static>,
    pin_down: Output<'static>,
    button_up: Input<'static>,
    button_down: Input<'static>,
) -> ! {
    let mut desk = SvenState::new(
        PulsePin::new(pin_up, true),
        PulsePin::new(pin_down, true),
        button_up,
        button_down,
    );

    desk.publish();
    info!("Desk controller ready");

    loop {
        match embassy_time::with_timeout(Duration::from_millis(10), COMMANDS.receive()).await {
            Ok(command) => {
                info!("HTTP command: {:?}", command);
                desk.apply(command).await;
            }
            Err(_) => {
                desk.poll_buttons().await;
            }
        }
    }
}
