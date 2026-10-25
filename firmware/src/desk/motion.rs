//! Open-loop motion model for the desk.
//!
//! The desk has no encoder, so travel is estimated from motor on-time. The
//! table below was measured with continuous runs. Points between measurements
//! are linearly interpolated, and travel past the last point is extrapolated
//! at the measured steady-state speed. Keeping the model in one place makes
//! the forward estimate (time -> distance) and the inverse estimate
//! (distance -> time) agree with each other.

/// Measured continuous-run points: `(elapsed_ms, travelled_mm)`.
///
/// The first second is slower than the rest because the motor ramps up, which
/// is why the points are interpolated instead of treated as a single slope.
pub const MS_TO_MM: &[(u32, u32)] = &[
    (0, 0),
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

/// Measured steady-state speed after the initial ramp.
pub const STEADY_MM_PER_S: u32 = 38;
const STEADY_PERIOD_MS: u32 = 1000;

/// Estimated distance travelled after `elapsed_ms` of continuous motor run.
pub fn mm_for_ms(elapsed_ms: u32) -> u32 {
    let mut lower = (0, 0);
    for &upper in MS_TO_MM {
        if elapsed_ms <= upper.0 {
            let span_ms = upper.0 - lower.0;
            if span_ms == 0 {
                return upper.1;
            }
            let span_mm = upper.1 - lower.1;
            let scaled = (elapsed_ms - lower.0) as u64 * span_mm as u64;
            return lower.1 + ((scaled + span_ms as u64 / 2) / span_ms as u64) as u32;
        }
        lower = upper;
    }

    let last = MS_TO_MM.last().copied().unwrap_or((0, 0));
    let extra_ms = elapsed_ms - last.0;
    last.1
        + ((extra_ms as u64 * STEADY_MM_PER_S as u64 + STEADY_PERIOD_MS as u64 / 2)
            / STEADY_PERIOD_MS as u64) as u32
}

/// Estimated motor on-time needed to travel `distance_mm` in one continuous
/// run. This is the inverse of [`mm_for_ms`], so the height estimate after a
/// move matches the distance that was requested.
pub fn ms_for_mm(distance_mm: u32) -> u32 {
    let mut lower = (0, 0);
    for &upper in MS_TO_MM {
        if distance_mm <= upper.1 {
            let span_mm = upper.1 - lower.1;
            if span_mm == 0 {
                return upper.0;
            }
            let span_ms = upper.0 - lower.0;
            let scaled = (distance_mm - lower.1) as u64 * span_ms as u64;
            return lower.0 + ((scaled + span_mm as u64 / 2) / span_mm as u64) as u32;
        }
        lower = upper;
    }

    let last = MS_TO_MM.last().copied().unwrap_or((0, 0));
    let extra_mm = distance_mm - last.1;
    last.0
        + ((extra_mm as u64 * STEADY_PERIOD_MS as u64 + STEADY_MM_PER_S as u64 / 2)
            / STEADY_MM_PER_S as u64) as u32
}

/// Extra motor time added to calibration homing so the desk settles against
/// the physical end stop. At speed this covers roughly 77 mm of accumulated
/// open-loop error.
pub const CALIBRATION_GUARD_MS: u32 = 2000;

/// A calibration homing pulse: direction, estimated distance, and motor
/// on-time including the end-stop guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomingPlan {
    pub down: bool,
    pub distance_mm: u32,
    pub duration_ms: u32,
}

/// Plans the pulse used to home the desk during calibration.
///
/// When `calibrated` is false the height estimate cannot be trusted, so the
/// plan covers the full travel range downward. Otherwise it targets the
/// nearest estimated end. `guard_ms` is added past that estimate so the desk
/// settles against the physical stop and clears accumulated open-loop error.
pub fn calibration_plan(
    calibrated: bool,
    height_mm: u32,
    min_height_mm: u32,
    max_height_mm: u32,
    guard_ms: u32,
) -> HomingPlan {
    let (down, distance_mm) = if !calibrated {
        (true, max_height_mm.saturating_sub(min_height_mm))
    } else {
        let to_bottom = height_mm.saturating_sub(min_height_mm);
        let to_top = max_height_mm.saturating_sub(height_mm);
        if to_bottom < to_top {
            (true, to_bottom)
        } else {
            (false, to_top)
        }
    };

    HomingPlan {
        down,
        distance_mm,
        duration_ms: ms_for_mm(distance_mm).saturating_add(guard_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u32 = 622;
    const MAX: u32 = 1274;

    #[test]
    fn uncalibrated_calibration_homes_down_over_the_full_range() {
        let plan = calibration_plan(false, 0, MIN, MAX, CALIBRATION_GUARD_MS);
        assert!(plan.down);
        assert_eq!(plan.distance_mm, MAX - MIN);
        assert_eq!(
            plan.duration_ms,
            ms_for_mm(MAX - MIN) + CALIBRATION_GUARD_MS
        );
    }

    #[test]
    fn calibration_targets_the_nearest_estimated_end() {
        let low = calibration_plan(true, 700, MIN, MAX, CALIBRATION_GUARD_MS);
        assert!(low.down);
        assert_eq!(low.distance_mm, 700 - MIN);

        let high = calibration_plan(true, 1200, MIN, MAX, CALIBRATION_GUARD_MS);
        assert!(!high.down);
        assert_eq!(high.distance_mm, MAX - 1200);

        // Equidistant heights round toward the top end.
        let mid = calibration_plan(true, (MIN + MAX) / 2, MIN, MAX, CALIBRATION_GUARD_MS);
        assert!(!mid.down);
    }

    #[test]
    fn calibration_guard_extends_every_homing_pulse() {
        for height in [MIN, 750, 948, 1140, MAX] {
            let plan = calibration_plan(true, height, MIN, MAX, CALIBRATION_GUARD_MS);
            assert_eq!(
                plan.duration_ms,
                ms_for_mm(plan.distance_mm) + CALIBRATION_GUARD_MS
            );
        }
    }

    #[test]
    fn measured_points_are_reproduced() {
        for &(ms, mm) in MS_TO_MM {
            assert_eq!(mm_for_ms(ms), mm, "{ms} ms should map to {mm} mm");
            assert_eq!(ms_for_mm(mm), ms, "{mm} mm should map to {ms} ms");
        }
    }

    #[test]
    fn inverse_round_trips_within_one_millimetre() {
        for distance in 0..=800 {
            let duration = ms_for_mm(distance);
            let round_trip = mm_for_ms(duration);
            assert!(
                round_trip.abs_diff(distance) <= 1,
                "{distance} mm -> {duration} ms -> {round_trip} mm"
            );
        }
    }

    #[test]
    fn duration_is_monotonic() {
        let mut previous = 0;
        for distance in 0..=800 {
            let duration = ms_for_mm(distance);
            assert!(
                duration >= previous,
                "{distance} mm produced {duration} ms, less than {previous} ms"
            );
            previous = duration;
        }
    }

    #[test]
    fn travel_is_monotonic() {
        let mut previous = 0;
        for elapsed in 0..=30000 {
            let distance = mm_for_ms(elapsed);
            assert!(
                distance >= previous,
                "{elapsed} ms produced {distance} mm, less than {previous} mm"
            );
            previous = distance;
        }
    }

    #[test]
    fn extrapolates_at_steady_speed() {
        assert_eq!(ms_for_mm(347 + STEADY_MM_PER_S), 11000);
        assert_eq!(mm_for_ms(11000), 347 + STEADY_MM_PER_S);
        assert_eq!(ms_for_mm(347 + 2 * STEADY_MM_PER_S), 12000);
    }

    #[test]
    fn zero_distance_does_not_need_a_pulse() {
        assert_eq!(ms_for_mm(0), 0);
        assert_eq!(mm_for_ms(0), 0);
    }

    #[test]
    fn common_relative_moves_are_close_to_the_table() {
        // The UI moves in 10 mm steps. Make sure a full-range move stays in
        // the measured envelope and does not fall back to the stale integer
        // second lookup.
        assert_eq!(ms_for_mm(10), 1026);
        assert_eq!(ms_for_mm(300), 8737);
        assert_eq!(ms_for_mm(500), 14026);
    }
}
