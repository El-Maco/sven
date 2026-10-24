use core::sync::atomic::{AtomicBool, Ordering};

use embassy_time::{Duration, Timer};
use esp_hal::gpio::Output;

/// Set by the HTTP `/api/stop` handler. Motion loops check it between short
/// pulses so a move can be interrupted without waiting for the full duration.
pub static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

const CHUNK_MS: u32 = 50;

pub struct PulsePin<'d> {
    pin: Output<'d>,
    active_high: bool,
}

impl<'d> PulsePin<'d> {
    pub fn new(pin: Output<'d>, active_high: bool) -> Self {
        Self { pin, active_high }
    }

    fn on(&mut self) {
        if self.active_high {
            self.pin.set_high();
        } else {
            self.pin.set_low();
        }
    }

    fn off(&mut self) {
        if self.active_high {
            self.pin.set_low();
        } else {
            self.pin.set_high();
        }
    }

    /// Drives the pin for up to `duration_ms`, checking the stop flag every
    /// `CHUNK_MS`. Returns the number of milliseconds actually elapsed. A
    /// return value smaller than `duration_ms` means the move was stopped.
    pub async fn pulse(&mut self, duration_ms: u32) -> u32 {
        self.on();

        let mut elapsed = 0;
        while elapsed < duration_ms {
            if STOP_REQUESTED.load(Ordering::SeqCst) {
                break;
            }

            let chunk = (duration_ms - elapsed).min(CHUNK_MS);
            Timer::after(Duration::from_millis(chunk as u64)).await;
            elapsed += chunk;
        }

        self.off();
        STOP_REQUESTED.store(false, Ordering::SeqCst);
        elapsed
    }

    pub fn toggle_on(&mut self) {
        self.on();
    }

    pub fn toggle_off(&mut self) {
        self.off();
    }
}

pub fn stop_requested() -> bool {
    STOP_REQUESTED.load(Ordering::SeqCst)
}

pub fn clear_stop() {
    STOP_REQUESTED.store(false, Ordering::SeqCst);
}
