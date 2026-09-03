//! Click zones and clock formatting for the in-window player.

/// Side / center click seek step.
pub const SEEK_SECS: f64 = 10.0;

/// Relative seek direction from a click on the video hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickZone {
    SeekBack,
    Pause,
    SeekFwd,
}

/// Left third seek back, center pause, right third seek forward.
#[must_use]
pub fn click_zone(x_ratio: f32) -> ClickZone {
    if x_ratio < 1.0 / 3.0 {
        return ClickZone::SeekBack;
    }
    if x_ratio < 2.0 / 3.0 {
        return ClickZone::Pause;
    }
    ClickZone::SeekFwd
}

/// `h:mm:ss` or `m:ss` from a playback position.
#[must_use]
pub fn format_clock(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return String::from("0:00");
    }
    let total = secs.round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours == 0 {
        return format!("{minutes}:{seconds:02}");
    }
    format!("{hours}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_thirds() {
        assert_eq!(click_zone(0.0), ClickZone::SeekBack);
        assert_eq!(click_zone(0.5), ClickZone::Pause);
        assert_eq!(click_zone(0.9), ClickZone::SeekFwd);
    }

    #[test]
    fn clock_formats() {
        assert_eq!(format_clock(0.0), "0:00");
        assert_eq!(format_clock(65.4), "1:05");
        assert_eq!(format_clock(3661.0), "1:01:01");
    }

    #[test]
    fn clock_survives_nan_infinity_and_negative() {
        assert_eq!(format_clock(f64::NAN), "0:00");
        assert_eq!(format_clock(f64::INFINITY), "0:00");
        assert_eq!(format_clock(f64::NEG_INFINITY), "0:00");
        assert_eq!(format_clock(-5.0), "0:00");
    }
}
