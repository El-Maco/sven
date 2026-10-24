use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::Input;
use log::info;
use serde::{Deserialize, Serialize};

use crate::desk::gpio::{clear_stop, stop_requested, PulsePin};

/// Snapshot of everything the UI needs. `height_mm` is an estimate based on
/// the open-loop motion table, and `calibrated` says whether that estimate has
/// been anchored to a known end of the desk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeskState {
    pub height_mm: u32,
    pub position: SvenPosition,
    pub moving: bool,
    pub target_mm: Option<u32>,
    pub calibrated: bool,
}

impl DeskState {
    pub const UNKNOWN: Self = Self {
        height_mm: 0,
        position: SvenPosition::Custom,
        moving: false,
        target_mm: None,
        calibrated: false,
    };
}

/// Up to eight concurrent readers: four web tasks for SSE plus short-lived
/// `/api/state` requests.
pub static STATE: Watch<CriticalSectionRawMutex, DeskState, 8> =
    Watch::new_with(DeskState::UNKNOWN);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SvenPosition {
    Bottom = 0,
    Top = 1,
    Armrest = 2,
    AboveArmrest = 3,
    Standing = 4,
    Custom = 5,
}

impl TryFrom<u32> for SvenPosition {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Bottom),
            1 => Ok(Self::Top),
            2 => Ok(Self::Armrest),
            3 => Ok(Self::AboveArmrest),
            4 => Ok(Self::Standing),
            5 => Ok(Self::Custom),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SvenCommand {
    UpDuration,
    DownDuration,
    UpRelative,
    DownRelative,
    AbsoluteHeight,
    Position,
    Calibrate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DeskCommand {
    pub command: SvenCommand,
    pub value: u32,
}

pub struct SvenState<'d> {
    pub height_mm: u32,
    pub position: SvenPosition,
    pub moving: bool,
    pub target_mm: Option<u32>,
    pub calibrated: bool,
    stopped: bool,
    pin_up: PulsePin<'d>,
    pin_down: PulsePin<'d>,
    button_up: Input<'d>,
    button_down: Input<'d>,
}

impl<'d> SvenState<'d> {
    pub const MIN_HEIGHT_MM: u32 = 622;
    pub const MAX_HEIGHT_MM: u32 = 1274;

    const POSITIONS_MM: &'static [(SvenPosition, u32)] = &[
        (SvenPosition::Bottom, Self::MIN_HEIGHT_MM),
        (SvenPosition::Armrest, 750),
        (SvenPosition::AboveArmrest, 795),
        (SvenPosition::Standing, 1140),
        (SvenPosition::Top, Self::MAX_HEIGHT_MM),
    ];

    /// Duration to distance calibration. These are millimetres, not
    /// centimetres, despite the historical name of this table.
    const MS_TO_MM: &'static [(u32, u32)] = &[
        (1000, 9),
        (2000, 48),
        (3000, 82),
        (4000, 119),
        (5000, 160),
        (6000, 194),
        (7000, 234),
        (8000, 272),
        (9000, 310),
        (10000, 347),
    ];

    pub fn new(
        pin_up: PulsePin<'d>,
        pin_down: PulsePin<'d>,
        button_up: Input<'d>,
        button_down: Input<'d>,
    ) -> Self {
        Self {
            height_mm: 0,
            position: SvenPosition::Custom,
            moving: false,
            target_mm: None,
            calibrated: false,
            stopped: false,
            pin_up,
            pin_down,
            button_up,
            button_down,
        }
    }

    pub fn snapshot(&self) -> DeskState {
        DeskState {
            height_mm: self.height_mm,
            position: self.position,
            moving: self.moving,
            target_mm: self.target_mm,
            calibrated: self.calibrated,
        }
    }

    pub fn publish(&self) {
        STATE.sender().send(self.snapshot());
    }

    pub fn get_position_mm(&self, position: SvenPosition) -> u32 {
        Self::POSITIONS_MM
            .iter()
            .find(|&&(pos, _)| pos == position)
            .map_or(Self::MIN_HEIGHT_MM, |&(_, height)| height)
    }

    fn get_position_from_height(&self) -> SvenPosition {
        const POS_THRESH: u32 = 5;
        let current = self.height_mm;
        Self::POSITIONS_MM
            .iter()
            .find(|&&(_, height)| {
                current < height.saturating_add(POS_THRESH)
                    && current > height.saturating_sub(POS_THRESH)
            })
            .map_or(SvenPosition::Custom, |&(pos, _)| pos)
    }

    fn get_duration_mm(&self, ms: u32) -> u32 {
        let seconds = ms / 1000;
        if seconds > 10 {
            return 347 + 38 * (seconds - 10);
        }

        Self::MS_TO_MM
            .iter()
            .find(|&&(duration, _)| (duration / 1000) == seconds)
            .map_or(0, |&(_, mm)| mm)
    }

    fn estimate_target(&self, command: DeskCommand) -> u32 {
        match command.command {
            SvenCommand::UpDuration => self
                .height_mm
                .saturating_add(self.get_duration_mm(command.value))
                .min(Self::MAX_HEIGHT_MM),
            SvenCommand::DownDuration => self
                .height_mm
                .saturating_sub(self.get_duration_mm(command.value))
                .max(Self::MIN_HEIGHT_MM),
            SvenCommand::UpRelative => self
                .height_mm
                .saturating_add(command.value)
                .min(Self::MAX_HEIGHT_MM),
            SvenCommand::DownRelative => self
                .height_mm
                .saturating_sub(command.value)
                .max(Self::MIN_HEIGHT_MM),
            SvenCommand::AbsoluteHeight => command
                .value
                .clamp(Self::MIN_HEIGHT_MM, Self::MAX_HEIGHT_MM),
            SvenCommand::Position => self.get_position_mm(
                SvenPosition::try_from(command.value).unwrap_or(SvenPosition::Armrest),
            ),
            SvenCommand::Calibrate => self.get_position_mm(
                SvenPosition::try_from(command.value).unwrap_or(SvenPosition::Armrest),
            ),
        }
    }

    pub async fn apply(&mut self, command: DeskCommand) {
        self.stopped = false;
        clear_stop();
        self.moving = true;
        self.target_mm = Some(self.estimate_target(command));
        self.publish();

        match command.command {
            SvenCommand::UpDuration => {
                info!("Moving up for {} ms", command.value);
                self.move_up(command.value).await;
            }
            SvenCommand::DownDuration => {
                info!("Moving down for {} ms", command.value);
                self.move_down(command.value).await;
            }
            SvenCommand::UpRelative => {
                info!("Moving up by {} mm", command.value);
                self.move_up_relative(command.value).await;
            }
            SvenCommand::DownRelative => {
                info!("Moving down by {} mm", command.value);
                self.move_down_relative(command.value).await;
            }
            SvenCommand::AbsoluteHeight => {
                info!("Setting absolute height to {} mm", command.value);
                self.move_to_height(command.value).await;
            }
            SvenCommand::Position => {
                let position =
                    SvenPosition::try_from(command.value).unwrap_or(SvenPosition::Armrest);
                info!("Setting position to {:?}", position);
                self.move_to_position(position).await;
            }
            SvenCommand::Calibrate => {
                info!("Calibrating at {:?}", command.value);
                self.calibrate(command.value).await;
            }
        }

        self.moving = false;
        self.target_mm = None;
        self.stopped = false;
        self.publish();
    }

    pub async fn move_up(&mut self, delta_ms: u32) {
        if self.stopped {
            return;
        }

        info!("Moving up {} ms", delta_ms);
        let elapsed = self.pin_up.pulse(delta_ms).await;
        let delta_mm = self.get_duration_mm(elapsed);
        self.height_mm = Self::MAX_HEIGHT_MM.min(self.height_mm.saturating_add(delta_mm));
        self.position = self.get_position_from_height();
        if elapsed < delta_ms {
            self.stopped = true;
        }
        self.publish();
    }

    pub async fn move_down(&mut self, delta_ms: u32) {
        if self.stopped {
            return;
        }

        info!("Moving down {} ms", delta_ms);
        let elapsed = self.pin_down.pulse(delta_ms).await;
        let delta_mm = self.get_duration_mm(elapsed);
        self.height_mm = Self::MIN_HEIGHT_MM.max(self.height_mm.saturating_sub(delta_mm));
        self.position = self.get_position_from_height();
        if elapsed < delta_ms {
            self.stopped = true;
        }
        self.publish();
    }

    pub async fn move_up_relative(&mut self, delta_mm: u32) {
        let mut distance_left = delta_mm;

        while distance_left > 0 && !self.stopped {
            let (max_duration, max_distance) = Self::MS_TO_MM
                .iter()
                .rev()
                .find(|&&(_, mm)| mm <= distance_left)
                .unwrap_or(&(0, 0));

            if *max_duration == 0 {
                break;
            }

            info!("Moving up {} mm equates to {} ms", delta_mm, max_duration);
            self.move_up(*max_duration).await;

            if self.stopped {
                break;
            }

            Timer::after(Duration::from_millis(1000)).await;
            distance_left = distance_left.saturating_sub(*max_distance);
        }
    }

    pub async fn move_down_relative(&mut self, delta_mm: u32) {
        let mut distance_left = delta_mm;

        while distance_left > 0 && !self.stopped {
            let (max_duration, max_distance) = Self::MS_TO_MM
                .iter()
                .rev()
                .find(|&&(_, mm)| mm <= distance_left)
                .unwrap_or(&(0, 0));

            if *max_duration == 0 {
                break;
            }

            info!("Moving down {} mm equates to {} ms", delta_mm, max_duration);
            self.move_down(*max_duration).await;

            if self.stopped {
                break;
            }

            Timer::after(Duration::from_millis(1000)).await;
            distance_left = distance_left.saturating_sub(*max_distance);
        }
    }

    pub async fn move_to_height(&mut self, height_mm: u32) {
        if self.stopped {
            return;
        }

        let target = height_mm.clamp(Self::MIN_HEIGHT_MM, Self::MAX_HEIGHT_MM);
        info!("Moving from height {} mm to {} mm", self.height_mm, target);

        if target == self.height_mm {
            info!("Already at height {} mm", target);
            return;
        }

        if target > self.height_mm {
            self.move_up_relative(target - self.height_mm).await;
        } else {
            self.move_down_relative(self.height_mm - target).await;
        }
    }

    pub async fn move_to_position(&mut self, position: SvenPosition) {
        if self.stopped {
            return;
        }

        match self.position {
            SvenPosition::Custom => match position {
                SvenPosition::Bottom => {
                    self.move_down_relative(self.height_mm.saturating_sub(Self::MIN_HEIGHT_MM))
                        .await;
                    self.move_down(5000).await;
                }
                SvenPosition::Top => {
                    self.move_up_relative(Self::MAX_HEIGHT_MM.saturating_sub(self.height_mm))
                        .await;
                    self.move_up(5000).await;
                }
                SvenPosition::Custom => {}
                _ => {
                    let target_height = self.get_position_mm(position);
                    info!(
                        "Moving to custom position {:?} with target height {} mm",
                        position, target_height
                    );
                    self.move_to_height(target_height).await;
                }
            },
            _ => {}
        }

        if self.stopped {
            return;
        }

        match self.position {
            SvenPosition::Top => match position {
                SvenPosition::Top => self.move_up(5000).await,
                SvenPosition::Standing => self.move_down(4300).await,
                SvenPosition::AboveArmrest => self.move_down(13500).await,
                SvenPosition::Armrest => self.move_down(14800).await,
                SvenPosition::Bottom => self.move_down(20000).await,
                _ => {}
            },
            SvenPosition::Armrest => match position {
                SvenPosition::Bottom => self.move_down(5000).await,
                SvenPosition::AboveArmrest => self.move_up(1920).await,
                SvenPosition::Standing => self.move_up(11000).await,
                SvenPosition::Top => self.move_up(16000).await,
                _ => {}
            },
            SvenPosition::AboveArmrest => match position {
                SvenPosition::Armrest => self.move_down(1900).await,
                SvenPosition::Bottom => self.move_down(7000).await,
                SvenPosition::Standing => self.move_up(9900).await,
                SvenPosition::Top => self.move_up(15000).await,
                _ => {}
            },
            SvenPosition::Standing => match position {
                SvenPosition::Armrest => self.move_down(10800).await,
                SvenPosition::AboveArmrest => self.move_down(9900).await,
                SvenPosition::Bottom => self.move_down(15000).await,
                SvenPosition::Top => self.move_up(5000).await,
                _ => {}
            },
            SvenPosition::Bottom => match position {
                SvenPosition::Armrest => self.move_up(4300).await,
                SvenPosition::AboveArmrest => self.move_up(5300).await,
                SvenPosition::Standing => self.move_up(15000).await,
                SvenPosition::Top => self.move_up(20000).await,
                _ => {}
            },
            _ => {}
        }

        if self.stopped {
            return;
        }

        self.position = position;
        self.height_mm = self.get_position_mm(position);
        self.publish();
    }

    async fn calibrate(&mut self, value: u32) {
        let target = SvenPosition::try_from(value).unwrap_or(SvenPosition::Armrest);
        let distance_to_bottom = self.height_mm.saturating_sub(Self::MIN_HEIGHT_MM);
        let distance_to_top = Self::MAX_HEIGHT_MM.saturating_sub(self.height_mm);

        if distance_to_bottom < distance_to_top {
            self.move_to_position(SvenPosition::Bottom).await;
        } else {
            self.move_to_position(SvenPosition::Top).await;
        }

        if self.stopped {
            return;
        }

        self.calibrated = true;
        self.publish();
        self.move_to_position(target).await;
    }

    /// Reads the physical buttons. Holding a button jogs the desk for as long
    /// as it is held. The web `/api/stop` flag also interrupts a button move.
    pub async fn poll_buttons(&mut self) {
        if self.button_up.is_high() {
            self.manual_move(true).await;
        }

        if self.button_down.is_high() {
            self.manual_move(false).await;
        }
    }

    async fn manual_move(&mut self, up: bool) {
        // Drop a stop request that arrived while the desk was idle so the
        // first physical button press is not ignored.
        clear_stop();

        if up {
            self.pin_up.toggle_on();
        } else {
            self.pin_down.toggle_on();
        }

        let started = Instant::now();
        loop {
            let held = if up {
                self.button_up.is_high()
            } else {
                self.button_down.is_high()
            };

            if !held || stop_requested() {
                break;
            }

            Timer::after(Duration::from_millis(10)).await;
        }

        if up {
            self.pin_up.toggle_off();
        } else {
            self.pin_down.toggle_off();
        }

        clear_stop();

        let elapsed = started.elapsed().as_millis() as u32;
        let delta_mm = self.get_duration_mm(elapsed);

        if up {
            self.height_mm = Self::MAX_HEIGHT_MM.min(self.height_mm.saturating_add(delta_mm));
        } else {
            self.height_mm = Self::MIN_HEIGHT_MM.max(self.height_mm.saturating_sub(delta_mm));
        }

        self.position = self.get_position_from_height();
        self.publish();
    }
}
