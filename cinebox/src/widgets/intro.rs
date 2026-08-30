//! Shared ease-out intro used by media, person, and torrents.

const DURATION: f64 = 0.48;

/// 0 at start, 1 when finished. Cubic ease-out.
#[must_use]
pub fn t(started: Option<f64>, now: f64) -> f32 {
    let Some(t0) = started else {
        return 1.0;
    };
    let t = ((now - t0) / DURATION).clamp(0.0, 1.0) as f32;
    1.0 - (1.0 - t).powi(3)
}

#[must_use]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

#[must_use]
pub fn running(started: Option<f64>, now: f64) -> bool {
    t(started, now) < 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_start_is_finished() {
        assert!((t(None, 0.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn start_frame_is_near_zero() {
        assert!(t(Some(1.0), 1.0) < 0.02);
    }

    #[test]
    fn after_duration_is_one() {
        assert!((t(Some(0.0), 1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn running_tracks_duration() {
        assert!(running(Some(0.0), 0.1));
        assert!(!running(Some(0.0), 1.0));
        assert!(!running(None, 0.0));
    }
}
