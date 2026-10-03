//! Where the picture lands, for players that leave scaling to the app.

use cinebox_core::VideoScale;

/// A rectangle in one unit, points or pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Area {
    /// `self` is in points; window pixels are `pixels_per_point` times as many.
    #[must_use]
    pub fn to_pixels(self, pixels_per_point: f32) -> PixelRect {
        let left = (self.x * pixels_per_point).round() as i32;
        let top = (self.y * pixels_per_point).round() as i32;
        let right = ((self.x + self.width) * pixels_per_point).round() as i32;
        let bottom = ((self.y + self.height) * pixels_per_point).round() as i32;

        PixelRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }
}

/// Window pixels, top-left origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The picture's place in `area` under `scale`, matching what mpv does with
/// `keepaspect`, `panscan` and `video-zoom`. It may reach past `area`, which
/// then crops it.
#[must_use]
pub fn fit_video(area: Area, video: [u32; 2], scale: VideoScale) -> Area {
    let [video_w, video_h] = video;
    let unknown = video_w == 0 || video_h == 0;
    if unknown || scale == VideoScale::Fill {
        return area;
    }

    if area.width <= 0.0 || area.height <= 0.0 {
        return area;
    }

    let video_aspect = video_w as f32 / video_h as f32;
    let area_aspect = area.width / area.height;

    // Fitting inside, a wider picture spans the width; covering, it spans the height.
    let wider = video_aspect > area_aspect;
    let spans_width = if scale == VideoScale::Expand { !wider } else { wider };

    let (width, height) = if spans_width {
        (area.width, area.width / video_aspect)
    } else {
        (area.height * video_aspect, area.height)
    };

    let zoom = zoom_factor(scale);
    let width = width * zoom;
    let height = height * zoom;

    Area {
        x: area.x + (area.width - width) / 2.0,
        y: area.y + (area.height - height) / 2.0,
        width,
        height,
    }
}

fn zoom_factor(scale: VideoScale) -> f32 {
    match scale {
        VideoScale::Zoom115 => 1.15,
        VideoScale::Zoom130 => 1.30,
        VideoScale::Default | VideoScale::Expand | VideoScale::Fill => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Area = Area {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.5
    }

    #[test]
    fn scope_picture_is_letterboxed_top_and_bottom() {
        let fitted = fit_video(SCREEN, [3840, 1606], VideoScale::Default);

        assert!(close(fitted.width, 1920.0));
        assert!(close(fitted.height, 803.0));
        assert!(close(fitted.y, 138.5));
    }

    #[test]
    fn narrow_picture_is_pillarboxed() {
        let fitted = fit_video(SCREEN, [1440, 1080], VideoScale::Default);

        assert!(close(fitted.height, 1080.0));
        assert!(close(fitted.width, 1440.0));
        assert!(close(fitted.x, 240.0));
    }

    #[test]
    fn expand_covers_the_screen_and_crops_the_sides() {
        let fitted = fit_video(SCREEN, [3840, 1606], VideoScale::Expand);

        assert!(close(fitted.height, 1080.0));
        assert!(fitted.width > 1920.0);
        assert!(fitted.x < 0.0);
    }

    #[test]
    fn fill_stretches_to_the_area() {
        assert_eq!(fit_video(SCREEN, [3840, 1606], VideoScale::Fill), SCREEN);
    }

    #[test]
    fn zoom_grows_the_fitted_picture_around_its_centre() {
        let fitted = fit_video(SCREEN, [1920, 1080], VideoScale::Zoom130);

        assert!(close(fitted.width, 1920.0 * 1.3));
        assert!(close(fitted.x + fitted.width / 2.0, 960.0));
        assert!(close(fitted.y + fitted.height / 2.0, 540.0));
    }

    #[test]
    fn unknown_size_keeps_the_whole_area() {
        assert_eq!(fit_video(SCREEN, [0, 0], VideoScale::Default), SCREEN);
    }

    #[test]
    fn pixels_round_the_edges_not_the_size() {
        let area = Area {
            x: 0.4,
            y: 0.0,
            width: 10.2,
            height: 5.0,
        };

        let pixels = area.to_pixels(2.0);

        assert_eq!(pixels, PixelRect { x: 1, y: 0, width: 20, height: 10 });
    }
}
