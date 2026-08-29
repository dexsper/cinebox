//! Chrome sizes and click zones. The HWND hole matches these logical heights.

/// Player top bar (back + title), logical pixels.
pub const HEADER_LOGICAL: f32 = 56.0;
/// Player bottom bar (time + buttons), logical pixels.
pub const FOOTER_LOGICAL: f32 = 72.0;
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

/// HWND `wid` as mpv expects on 64-bit Windows.
#[must_use]
pub fn wid_from_hwnd(hwnd: isize) -> i64 {
    hwnd as u32 as i64
}

/// Child window rectangle inside the parent client area, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Video hole between header and footer. `dpi` is win32 DPI (96 = 100%).
#[must_use]
pub fn video_rect(client_w: i32, client_h: i32, dpi: u32) -> PixelRect {
    let scale = dpi.max(1) as f32 / 96.0;
    let header = (HEADER_LOGICAL * scale).round() as i32;
    let footer = (FOOTER_LOGICAL * scale).round() as i32;
    let y = header.clamp(0, client_h.max(0));
    let h = (client_h - header - footer).max(0);
    PixelRect {
        x: 0,
        y,
        w: client_w.max(0),
        h,
    }
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
    fn wid_truncates_like_mpv_win32() {
        let high = 0x0000_0001_8000_0000u64 as isize;
        assert_eq!(wid_from_hwnd(high), 0x8000_0000u32 as i64);
    }

    #[test]
    fn click_thirds() {
        assert_eq!(click_zone(0.0), ClickZone::SeekBack);
        assert_eq!(click_zone(0.5), ClickZone::Pause);
        assert_eq!(click_zone(0.9), ClickZone::SeekFwd);
    }

    #[test]
    fn hole_sits_between_chrome() {
        let rect = video_rect(1920, 1080, 96);
        assert_eq!(rect.y, HEADER_LOGICAL as i32);
        assert_eq!(rect.h, 1080 - HEADER_LOGICAL as i32 - FOOTER_LOGICAL as i32);
        assert_eq!(rect.w, 1920);
    }

    #[test]
    fn clock_formats() {
        assert_eq!(format_clock(0.0), "0:00");
        assert_eq!(format_clock(65.4), "1:05");
        assert_eq!(format_clock(3661.0), "1:01:01");
    }
}
