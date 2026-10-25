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

#[cfg(test)]
mod tests {
    use super::*;

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
