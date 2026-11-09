use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::Input;
use heapless::{String, Vec};
use log::info;
use serde::{Deserialize, Serialize};

use crate::desk::gpio::{clear_stop, stop_requested, PulsePin};
use crate::desk::motion::{calibration_plan, mm_for_ms, ms_for_mm, CALIBRATION_GUARD_MS};

pub const MIN_HEIGHT_MM: u32 = 622;
pub const MAX_HEIGHT_MM: u32 = 1274;
pub const POSITION_NAME_MAX: usize = 24;
pub const MAX_POSITIONS: usize = 8;

const POSITION_THRESHOLD_MM: u32 = 5;

pub type PositionName = String<POSITION_NAME_MAX>;
pub type Positions = Vec<PositionEntry, MAX_POSITIONS>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PositionEntry {
    pub name: PositionName,
    pub height_mm: u32,
}

/// User-defined presets loaded from MQTT. Bottom and Top are built-in ends,
/// not entries in this list.
static POSITIONS: BlockingMutex<CriticalSectionRawMutex, RefCell<Positions>> =
    BlockingMutex::new(RefCell::new(Vec::new()));
static POSITIONS_LOADED: AtomicBool = AtomicBool::new(false);
pub static POSITIONS_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

fn name(value: &str) -> PositionName {
    let mut result = PositionName::new();
    result.push_str(value).unwrap();
    result
}

pub fn seed_positions() -> Positions {
    let mut positions = Positions::new();
    for (name, height_mm) in [("Armrest", 750), ("AboveArmrest", 795), ("Standing", 1140)] {
        positions
            .push(PositionEntry {
                name: self::name(name),
                height_mm,
            })
            .ok();
    }
    positions
}

pub fn positions() -> Positions {
    POSITIONS.lock(|positions| positions.borrow().clone())
}

pub fn positions_loaded() -> bool {
    POSITIONS_LOADED.load(Ordering::SeqCst)
}

fn mark_positions_changed() {
    POSITIONS_LOADED.store(true, Ordering::SeqCst);
    POSITIONS_CHANGED.signal(());
}

pub fn replace_positions(next: Positions) {
    POSITIONS.lock(|positions| *positions.borrow_mut() = next);
    mark_positions_changed();
}

pub fn ensure_seeded_positions() {
    if !positions_loaded() {
        replace_positions(seed_positions());
    }
}

fn valid_position_name(value: &str) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("position name is required");
    }
    if value.len() > POSITION_NAME_MAX {
        return Err("position name is too long");
    }
    if matches!(value, "Bottom" | "Top" | "Custom") {
        return Err("position name is reserved");
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'))
    {
        return Err("position name has unsupported characters");
    }
    Ok(())
}

pub fn sanitize_positions(list: Positions) -> Positions {
    let mut result = Positions::new();
    for entry in list {
        if valid_position_name(entry.name.as_str()).is_err()
            || !(MIN_HEIGHT_MM..=MAX_HEIGHT_MM).contains(&entry.height_mm)
            || result.iter().any(|existing| existing.name == entry.name)
        {
            continue;
        }
        result.push(entry).ok();
    }
    result
}

pub fn add_position(name: PositionName, height_mm: u32) -> Result<(), &'static str> {
    valid_position_name(name.as_str())?;
    if !(MIN_HEIGHT_MM..=MAX_HEIGHT_MM).contains(&height_mm) {
        return Err("position height is out of range");
    }

    let result = POSITIONS.lock(|positions| -> Result<(), &'static str> {
        let mut positions = positions.borrow_mut();
        if let Some(existing) = positions.iter_mut().find(|entry| entry.name == name) {
            existing.height_mm = height_mm;
            return Ok(());
        }
        positions
            .push(PositionEntry { name, height_mm })
            .map_err(|_| "too many positions")
    });

    if result.is_ok() {
        mark_positions_changed();
    }
    result
}

pub fn remove_position(name: &str) -> Result<(), &'static str> {
    let result = POSITIONS.lock(|positions| -> Result<(), &'static str> {
        let mut positions = positions.borrow_mut();
        let index = positions
            .iter()
            .position(|entry| entry.name == name)
            .ok_or("position not found")?;
        positions.remove(index);
        Ok(())
    });

    if result.is_ok() {
        mark_positions_changed();
    }
    result
}

pub fn position_height(index: u32) -> Option<u32> {
    match index {
        0 => Some(MIN_HEIGHT_MM),
        1 => Some(MAX_HEIGHT_MM),
        _ => positions()
            .get(index as usize - 2)
            .map(|entry| entry.height_mm),
    }
}

pub fn position_name_for_height(height_mm: u32) -> PositionName {
    if !(MIN_HEIGHT_MM..=MAX_HEIGHT_MM).contains(&height_mm) {
        return name("Custom");
    }
    if height_mm <= MIN_HEIGHT_MM.saturating_add(POSITION_THRESHOLD_MM) {
        return name("Bottom");
    }
    if height_mm >= MAX_HEIGHT_MM.saturating_sub(POSITION_THRESHOLD_MM) {
        return name("Top");
    }

    positions()
        .iter()
        .find(|entry| entry.height_mm.abs_diff(height_mm) <= POSITION_THRESHOLD_MM)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| name("Custom"))
}

/// Snapshot of everything the UI needs. `height_mm` is an estimate based on
/// the open-loop motion table, and `calibrated` says whether that estimate has
/// been anchored to a known end of the desk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeskState {
    pub height_mm: u32,
    pub position: PositionName,
    pub moving: bool,
    pub target_mm: Option<u32>,
    pub calibrated: bool,
}

impl DeskState {
    pub const UNKNOWN: Self = Self {
        height_mm: 0,
        position: String::new(),
        moving: false,
        target_mm: None,
        calibrated: false,
    };
}

/// Concurrent readers: four web tasks for SSE, short-lived `/api/state`
/// requests, and the MQTT publisher.
pub static STATE: Watch<CriticalSectionRawMutex, DeskState, 16> =
    Watch::new_with(DeskState::UNKNOWN);

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
    pub position: PositionName,
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
    /// Extra travel folded into an end-stop move so it reaches the physical
    /// limit even when the height estimate is slightly off.
    const END_GUARD_MS: u32 = 5000;

    pub fn new(
        pin_up: PulsePin<'d>,
        pin_down: PulsePin<'d>,
        button_up: Input<'d>,
        button_down: Input<'d>,
        restored_height_mm: u32,
        restored_calibrated: bool,
    ) -> Self {
        let calibrated =
            restored_calibrated && (MIN_HEIGHT_MM..=MAX_HEIGHT_MM).contains(&restored_height_mm);
        let height_mm = if calibrated { restored_height_mm } else { 0 };

        Self {
            height_mm,
            position: if calibrated {
                position_name_for_height(height_mm)
            } else {
                name("Custom")
            },
            moving: false,
            target_mm: None,
            calibrated,
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
            position: self.position.clone(),
            moving: self.moving,
            target_mm: self.target_mm,
            calibrated: self.calibrated,
        }
    }

    pub fn publish(&self) {
        STATE.sender().send(self.snapshot());
    }

    fn get_position_from_height(&self) -> PositionName {
        position_name_for_height(self.height_mm)
    }

    fn estimate_target(&self, command: DeskCommand) -> u32 {
        match command.command {
            SvenCommand::UpDuration => self
                .height_mm
                .saturating_add(mm_for_ms(command.value))
                .min(MAX_HEIGHT_MM),
            SvenCommand::DownDuration => self
                .height_mm
                .saturating_sub(mm_for_ms(command.value))
                .max(MIN_HEIGHT_MM),
            SvenCommand::UpRelative => self
                .height_mm
                .saturating_add(command.value)
                .min(MAX_HEIGHT_MM),
            SvenCommand::DownRelative => self
                .height_mm
                .saturating_sub(command.value)
                .max(MIN_HEIGHT_MM),
            SvenCommand::AbsoluteHeight => command.value.clamp(MIN_HEIGHT_MM, MAX_HEIGHT_MM),
            SvenCommand::Position | SvenCommand::Calibrate => {
                position_height(command.value).unwrap_or(self.height_mm)
            }
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
                info!("Setting position to index {}", command.value);
                self.move_to_position(command.value).await;
            }
            SvenCommand::Calibrate => {
                info!("Calibrating at index {}", command.value);
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
        let delta_mm = mm_for_ms(elapsed);
        self.height_mm = MAX_HEIGHT_MM.min(self.height_mm.saturating_add(delta_mm));
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
        let delta_mm = mm_for_ms(elapsed);
        self.height_mm = MIN_HEIGHT_MM.max(self.height_mm.saturating_sub(delta_mm));
        self.position = self.get_position_from_height();
        if elapsed < delta_ms {
            self.stopped = true;
        }
        self.publish();
    }

    pub async fn move_up_relative(&mut self, delta_mm: u32) {
        self.move_relative(delta_mm, true).await;
    }

    pub async fn move_down_relative(&mut self, delta_mm: u32) {
        self.move_relative(delta_mm, false).await;
    }

    /// Moves by an estimated distance in one continuous motor pulse.
    ///
    /// The measured table describes travel for a continuous run, so the entire
    /// distance is converted to one duration up front. Splitting the move into
    /// shorter pulses would restart the motor's ramp each time, which makes the
    /// open-loop estimate worse rather than more precise.
    async fn move_relative(&mut self, delta_mm: u32, up: bool) {
        if self.stopped {
            return;
        }

        let available = if up {
            MAX_HEIGHT_MM.saturating_sub(self.height_mm)
        } else {
            self.height_mm.saturating_sub(MIN_HEIGHT_MM)
        };
        let delta_mm = delta_mm.min(available);
        if delta_mm == 0 {
            return;
        }

        let duration_ms = ms_for_mm(delta_mm);
        info!(
            "Moving {} mm {} in one {} ms pulse",
            delta_mm,
            if up { "up" } else { "down" },
            duration_ms
        );

        if up {
            self.move_up(duration_ms).await;
        } else {
            self.move_down(duration_ms).await;
        }
    }

    pub async fn move_to_height(&mut self, height_mm: u32) {
        if self.stopped {
            return;
        }

        let target = height_mm.clamp(MIN_HEIGHT_MM, MAX_HEIGHT_MM);
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

    pub async fn move_to_position(&mut self, index: u32) {
        if self.stopped {
            return;
        }

        match index {
            0 => {
                let distance_mm = self.height_mm.saturating_sub(MIN_HEIGHT_MM);
                let duration_ms = ms_for_mm(distance_mm).saturating_add(Self::END_GUARD_MS);
                info!(
                    "Moving to bottom from estimated {} mm in one {} ms pulse",
                    self.height_mm, duration_ms
                );
                self.move_down(duration_ms).await;
            }
            1 => {
                let distance_mm = MAX_HEIGHT_MM.saturating_sub(self.height_mm);
                let duration_ms = ms_for_mm(distance_mm).saturating_add(Self::END_GUARD_MS);
                info!(
                    "Moving to top from estimated {} mm in one {} ms pulse",
                    self.height_mm, duration_ms
                );
                self.move_up(duration_ms).await;
            }
            _ => {
                let Some(target_height) = position_height(index) else {
                    return;
                };
                info!(
                    "Moving to position index {} with target height {} mm",
                    index, target_height
                );
                self.move_to_height(target_height).await;
                if self.stopped {
                    return;
                }
                self.height_mm = target_height;
                self.position = position_name_for_height(target_height);
                self.publish();
            }
        }
    }

    async fn calibrate(&mut self, value: u32) {
        let plan = calibration_plan(
            self.calibrated,
            self.height_mm,
            MIN_HEIGHT_MM,
            MAX_HEIGHT_MM,
            CALIBRATION_GUARD_MS,
        );
        let end_index = if plan.down { 0 } else { 1 };

        info!(
            "Calibrating toward index {}: homing {} over {} mm in one {} ms pulse ({} ms guard)",
            value, end_index, plan.distance_mm, plan.duration_ms, CALIBRATION_GUARD_MS
        );

        if plan.down {
            self.move_down(plan.duration_ms).await;
        } else {
            self.move_up(plan.duration_ms).await;
        }

        if self.stopped {
            return;
        }

        self.height_mm = if plan.down {
            MIN_HEIGHT_MM
        } else {
            MAX_HEIGHT_MM
        };
        self.position = position_name_for_height(self.height_mm);
        self.publish();

        self.calibrated = true;
        self.publish();

        if value != end_index {
            self.move_to_position(value).await;
        }
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
        let delta_mm = mm_for_ms(elapsed);

        if up {
            self.height_mm = MAX_HEIGHT_MM.min(self.height_mm.saturating_add(delta_mm));
        } else {
            self.height_mm = MIN_HEIGHT_MM.max(self.height_mm.saturating_sub(delta_mm));
        }

        self.position = self.get_position_from_height();
        self.publish();
    }
}
