//! Soften a TMDB backdrop and paint it as a full-window wallpaper.

use std::io::Cursor;

use egui::{Color32, Mesh, Pos2, Rect, TextureHandle, Ui, epaint::Vertex};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use thiserror::Error;

use crate::theme::{self, Theme};

const CENTER_VEIL: f32 = 0.28;
const EDGE_VEIL: f32 = 0.74;
const BOTTOM_BLUR_START: f32 = 0.60;
const LEFT_BLUR_END: f32 = 0.46;
const BLUR_SIGMA: f32 = 14.0;
const LEFT_FADE_SHARE: f32 = 0.42;
const BOTTOM_FADE_SHARE: f32 = 0.45;
const ACCENT_SAMPLES_PER_SIDE: u32 = 32;
const MIN_ACCENT_WEIGHT: f32 = 0.12;
const WASH_LUMINANCE: f32 = 0.14;
const WASH_HUE_SHARE: f32 = 0.62;
const QUAD_INDICES: [u32; 6] = [0, 1, 2, 0, 2, 3];

#[derive(Debug, Error)]
pub(crate) enum SoftenError {
    #[error("cannot decode backdrop")]
    Decode(#[source] image::ImageError),

    #[error("backdrop of {width}x{height} is too small to soften")]
    TooSmall { width: u32, height: u32 },

    #[error("cannot encode softened backdrop")]
    Encode(#[source] image::ImageError),
}

/// Cover-fit wallpaper plus left/bottom page-bg fades.
pub(crate) fn paint(ui: &mut Ui, texture: &TextureHandle, theme: &Theme) {
    let rect = ui.max_rect();
    paint_cover(ui, texture, rect);
    paint_left_fade(ui, rect, theme.page_bg);
    paint_bottom_fade(ui, rect, theme.page_bg);
}

fn paint_cover(ui: &Ui, texture: &TextureHandle, rect: Rect) {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }

    let scale = (rect.width() / size.x).max(rect.height() / size.y);
    let drawn = size * scale;
    let image_rect = Rect::from_center_size(rect.center(), drawn);
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    ui.painter().with_clip_rect(rect).image(texture.id(), image_rect, uv, Color32::WHITE);
}

fn paint_left_fade(ui: &Ui, rect: Rect, page: Color32) {
    let width = rect.width() * LEFT_FADE_SHARE;
    let band = Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + width, rect.bottom()));
    let (opaque, clear) = (with_alpha(page, 97), with_alpha(page, 0));
    paint_gradient(ui, band, [opaque, clear, clear, opaque]);
}

fn paint_bottom_fade(ui: &Ui, rect: Rect, page: Color32) {
    let height = rect.height() * BOTTOM_FADE_SHARE;
    let band = Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - height), rect.right_bottom());
    let (clear, opaque) = (with_alpha(page, 0), with_alpha(page, 107));
    paint_gradient(ui, band, [clear, clear, opaque, opaque]);
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    let [red, green, blue, _] = color.to_array();
    Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

/// `colors` go clockwise from the top-left corner.
fn paint_gradient(ui: &Ui, rect: Rect, colors: [Color32; 4]) {
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    let mut mesh = Mesh::default();

    for (pos, color) in corners.into_iter().zip(colors) {
        mesh.vertices.push(Vertex { pos, uv: Pos2::ZERO, color });
    }

    mesh.indices.extend_from_slice(&QUAD_INDICES);
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// Blur the left and bottom edges and veil the frame with a darkened accent from the image.
pub(crate) fn soften(bytes: &[u8]) -> Result<Vec<u8>, SoftenError> {
    let sharp = image::load_from_memory(bytes).map_err(SoftenError::Decode)?.to_rgba8();
    let (width, height) = sharp.dimensions();

    if width < 2 || height < 2 {
        return Err(SoftenError::TooSmall { width, height });
    }

    let accent = accent_wash(&sharp);
    let blurred = image::imageops::fast_blur(&sharp, BLUR_SIGMA);
    let out = RgbaImage::from_fn(width, height, |x, y| {
        let left = 1.0 - smoothstep(0.0, LEFT_BLUR_END, unit(x, width));
        let bottom = smoothstep(BOTTOM_BLUR_START, 1.0, unit(y, height));
        let edge = 1.0 - (1.0 - left) * (1.0 - bottom);
        let veil = CENTER_VEIL + edge * EDGE_VEIL;
        let sharp_pixel = sharp.get_pixel(x, y).0;
        let soft_pixel = blurred.get_pixel(x, y).0;
        let channel = |index: usize| {
            let mixed = lerp(sharp_pixel[index], soft_pixel[index], edge);
            lerp(mixed, accent[index], veil)
        };

        Rgba([channel(0), channel(1), channel(2), u8::MAX])
    });

    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(out).write_to(&mut encoded, ImageFormat::Png).map_err(SoftenError::Encode)?;

    Ok(encoded.into_inner())
}

/// Where `index` sits between the first and the last of `count` pixels, from 0 to 1.
#[expect(clippy::cast_precision_loss, reason = "image sides stay below the 2^24 that `f32` holds exactly")]
fn unit(index: u32, count: u32) -> f32 {
    index as f32 / (count - 1) as f32
}

fn accent_wash(src: &RgbaImage) -> [u8; 3] {
    let Some(rgb) = accent_color(src) else { return theme::PAGE_BG_RGB };

    dark_wash(rgb)
}

/// The mean of saturated mid-tone pixels, or of all pixels when the image is nearly gray.
fn accent_color(src: &RgbaImage) -> Option<[f32; 3]> {
    let (width, height) = src.dimensions();
    let step_x = (width / ACCENT_SAMPLES_PER_SIDE).max(1);
    let step_y = (height / ACCENT_SAMPLES_PER_SIDE).max(1);
    let mut chroma = Average::default();
    let mut mean = Average::default();

    for y in (0..height).step_by(step_y as usize) {
        for x in (0..width).step_by(step_x as usize) {
            let [red, green, blue, alpha] = src.get_pixel(x, y).0;
            if alpha < 16 {
                continue;
            }

            let rgb = [red, green, blue].map(|channel| f32::from(channel) / 255.0);
            mean.add(rgb, 1.0);
            chroma.add(rgb, chroma_weight(rgb));
        }
    }

    if chroma.weight > MIN_ACCENT_WEIGHT {
        return chroma.get();
    }

    mean.get()
}

fn chroma_weight(rgb: [f32; 3]) -> f32 {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let saturation = if max > 1e-4 { (max - min) / max } else { 0.0 };
    let luminance = luminance(rgb);
    let mid_tone = (luminance * (1.0 - luminance) * 4.0).clamp(0.05, 1.0);

    saturation * saturation * mid_tone
}

#[derive(Default)]
struct Average {
    sum: [f32; 3],
    weight: f32,
}

impl Average {
    fn add(&mut self, rgb: [f32; 3], weight: f32) {
        for (sum, channel) in self.sum.iter_mut().zip(rgb) {
            *sum += channel * weight;
        }

        self.weight += weight;
    }

    fn get(&self) -> Option<[f32; 3]> {
        if self.weight <= 0.0 {
            return None;
        }

        Some(self.sum.map(|sum| sum / self.weight))
    }
}

fn dark_wash(rgb: [f32; 3]) -> [u8; 3] {
    let luminance = luminance(rgb);
    let scale = if luminance > 1e-4 { (WASH_LUMINANCE / luminance).min(1.0) } else { 0.0 };
    let gray = WASH_LUMINANCE * (1.0 - WASH_HUE_SHARE);

    rgb.map(|channel| to_u8(channel * scale * WASH_HUE_SHARE + gray))
}

fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// A share from 0 to 1 as a channel byte.
fn to_u8(share: f32) -> u8 {
    round_to_u8(share * 255.0)
}

fn lerp(from: u8, to: u8, share: f32) -> u8 {
    let share = share.clamp(0.0, 1.0);
    let span = f32::from(to) - f32::from(from);

    round_to_u8(f32::from(from) + span * share)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to the `u8` range first"
)]
fn round_to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let share = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);

    share * share * (3.0 - 2.0 * share)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(pixel: [u8; 4]) -> i32 {
        i32::from(pixel[0]) * 3 + i32::from(pixel[1]) * 6 + i32::from(pixel[2])
    }

    fn png_bytes(src: RgbaImage) -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(src).write_to(&mut encoded, ImageFormat::Png).unwrap();
        encoded.into_inner()
    }

    fn soften_rgba(src: RgbaImage) -> RgbaImage {
        let softened = soften(&png_bytes(src)).unwrap();
        image::load_from_memory(&softened).unwrap().to_rgba8()
    }

    #[test]
    fn left_and_bottom_edges_darker_than_center() {
        let out = soften_rgba(RgbaImage::from_pixel(48, 48, Rgba([255, 255, 255, 255])));

        let highlight = out.get_pixel(44, 4).0;
        let left = out.get_pixel(2, 24).0;
        let bottom = out.get_pixel(24, 45).0;

        assert!(luma(left) < luma(highlight), "left {left:?} should be darker than top-right {highlight:?}");
        assert!(luma(bottom) < luma(highlight), "bottom {bottom:?} should be darker than top-right {highlight:?}");
    }

    #[test]
    fn center_is_slightly_muted_and_edges_keep_accent_hue() {
        let out = soften_rgba(RgbaImage::from_pixel(48, 48, Rgba([220, 36, 48, 255])));

        let center = out.get_pixel(40, 8).0;
        let left = out.get_pixel(2, 24).0;

        assert!(center[0] < 220 && center[0] > 120, "center should be slightly muted, got {center:?}");
        assert!(i16::from(center[0]) - i16::from(center[1]) > 40, "center should stay red-tinted, got {center:?}");
        assert!(luma(left) < luma(center), "left edge {left:?} should be a stronger veil than center {center:?}");
        assert!(i16::from(left[0]) - i16::from(left[1]) > 20, "edge veil should keep the image accent, got {left:?}");
    }

    #[test]
    fn rejects_a_one_pixel_backdrop() {
        let bytes = png_bytes(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255])));

        let result = soften(&bytes);

        assert!(matches!(result, Err(SoftenError::TooSmall { width: 1, height: 1 })));
    }
}
