//! Soften a TMDB backdrop and paint it as a full-window wallpaper.

use std::io::Cursor;

use egui::{Color32, Mesh, Pos2, Rect, TextureHandle, Ui, epaint::Vertex};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use crate::theme::Theme;

const PAGE_BG_RGB: [u8; 3] = [0x2B, 0x2D, 0x31];
const CENTER_VEIL: f32 = 0.09;
const EDGE_VEIL: f32 = 0.74;

/// Cover-fit wallpaper plus left/bottom page-bg fades.
pub fn paint(ui: &mut Ui, texture: &TextureHandle, theme: &Theme) {
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
    ui.painter()
        .with_clip_rect(rect)
        .image(texture.id(), image_rect, uv, Color32::WHITE);
}

fn paint_left_fade(ui: &Ui, rect: Rect, page: Color32) {
    let w = rect.width() * 0.42;
    let band = Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + w, rect.bottom()));
    gradient_h(ui, band, with_a(page, 97), with_a(page, 0));
}

fn paint_bottom_fade(ui: &Ui, rect: Rect, page: Color32) {
    let h = rect.height() * 0.45;
    let band = Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - h), rect.right_bottom());
    gradient_v(ui, band, with_a(page, 0), with_a(page, 107));
}

fn with_a(color: Color32, a: u8) -> Color32 {
    let [r, g, b, _] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn gradient_h(ui: &Ui, rect: Rect, left: Color32, right: Color32) {
    let mut mesh = Mesh::default();
    let i = mesh.vertices.len() as u32;

    mesh.vertices.push(vert(rect.left_top(), left));
    mesh.vertices.push(vert(rect.right_top(), right));
    mesh.vertices.push(vert(rect.right_bottom(), right));
    mesh.vertices.push(vert(rect.left_bottom(), left));
    mesh.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn gradient_v(ui: &Ui, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    let i = mesh.vertices.len() as u32;

    mesh.vertices.push(vert(rect.left_top(), top));
    mesh.vertices.push(vert(rect.right_top(), top));
    mesh.vertices.push(vert(rect.right_bottom(), bottom));
    mesh.vertices.push(vert(rect.left_bottom(), bottom));
    mesh.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn vert(pos: Pos2, color: Color32) -> Vertex {
    Vertex {
        pos,
        uv: Pos2::ZERO,
        color,
    }
}

/// Blur the left and bottom edges and veil the frame with a darkened accent from the image.
///
/// # Errors
///
/// Decode or PNG encode failure.
pub fn soften(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let rgba = image::load_from_memory(bytes)
        .map_err(|error| error.to_string())?
        .to_rgba8();

    let (width, height) = rgba.dimensions();
    if width < 2 || height < 2 {
        return Err(String::from("backdrop too small"));
    }

    let accent = accent_wash(&rgba);
    let blurred = image::imageops::fast_blur(&rgba, 14.0);
    let mut out = RgbaImage::new(width, height);
    let inv_x = 1.0 / (width - 1) as f32;
    let inv_y = 1.0 / (height - 1) as f32;
    for y in 0..height {
        let ny = y as f32 * inv_y;
        let bottom = smoothstep(0.52, 1.0, ny);
        for x in 0..width {
            let nx = x as f32 * inv_x;
            let left = 1.0 - smoothstep(0.0, 0.46, nx);
            let edge = 1.0 - (1.0 - left) * (1.0 - bottom);
            let sharp = rgba.get_pixel(x, y).0;
            let soft = blurred.get_pixel(x, y).0;
            let mixed = [
                lerp(sharp[0], soft[0], edge),
                lerp(sharp[1], soft[1], edge),
                lerp(sharp[2], soft[2], edge),
                255,
            ];

            let veil = CENTER_VEIL + edge * EDGE_VEIL;
            out.put_pixel(
                x,
                y,
                Rgba([
                    lerp(mixed[0], accent[0], veil),
                    lerp(mixed[1], accent[1], veil),
                    lerp(mixed[2], accent[2], veil),
                    255,
                ]),
            );
        }
    }

    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(out)
        .write_to(&mut encoded, ImageFormat::Png)
        .map_err(|error| error.to_string())?;

    Ok(encoded.into_inner())
}

fn accent_wash(src: &RgbaImage) -> [u8; 3] {
    let (width, height) = src.dimensions();
    let step_x = (width / 32).max(1);
    let step_y = (height / 32).max(1);
    let mut chroma = [0.0f32; 3];
    let mut chroma_w = 0.0f32;
    let mut mean = [0.0f32; 3];
    let mut count = 0.0f32;

    for y in (0..height).step_by(step_y as usize) {
        for x in (0..width).step_by(step_x as usize) {
            let [r, g, b, a] = src.get_pixel(x, y).0;
            if a < 16 {
                continue;
            }

            let rf = f32::from(r) / 255.0;
            let gf = f32::from(g) / 255.0;
            let bf = f32::from(b) / 255.0;
            mean[0] += rf;
            mean[1] += gf;
            mean[2] += bf;
            count += 1.0;

            let max = rf.max(gf).max(bf);
            let min = rf.min(gf).min(bf);
            let sat = if max > 1e-4 { (max - min) / max } else { 0.0 };
            let lum = 0.2126 * rf + 0.7152 * gf + 0.0722 * bf;
            let mid = (lum * (1.0 - lum) * 4.0).clamp(0.05, 1.0);
            let weight = sat * sat * mid;

            chroma[0] += rf * weight;
            chroma[1] += gf * weight;
            chroma[2] += bf * weight;
            chroma_w += weight;
        }
    }
    let rgb = if chroma_w > 0.12 {
        [
            chroma[0] / chroma_w,
            chroma[1] / chroma_w,
            chroma[2] / chroma_w,
        ]
    } else if count > 0.0 {
        [mean[0] / count, mean[1] / count, mean[2] / count]
    } else {
        return PAGE_BG_RGB;
    };
    dark_wash(rgb)
}

fn dark_wash(rgb: [f32; 3]) -> [u8; 3] {
    let lum = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    let target_lum = 0.14;
    let scaled = if lum > 1e-4 {
        let scale = (target_lum / lum).min(1.0);
        [rgb[0] * scale, rgb[1] * scale, rgb[2] * scale]
    } else {
        [0.0, 0.0, 0.0]
    };

    let gray = target_lum;
    let keep = 0.62;
    channel_u8([
        scaled[0] * keep + gray * (1.0 - keep),
        scaled[1] * keep + gray * (1.0 - keep),
        scaled[2] * keep + gray * (1.0 - keep),
    ])
}

fn channel_u8(rgb: [f32; 3]) -> [u8; 3] {
    [
        (rgb[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (rgb[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (rgb[2] * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(px: [u8; 4]) -> i32 {
        i32::from(px[0]) * 3 + i32::from(px[1]) * 6 + i32::from(px[2])
    }

    fn png_bytes(src: RgbaImage) -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        match DynamicImage::ImageRgba8(src).write_to(&mut encoded, ImageFormat::Png) {
            Ok(()) => encoded.into_inner(),
            Err(error) => panic!("{error}"),
        }
    }

    fn soften_rgba(src: RgbaImage) -> RgbaImage {
        let softened = match soften(&png_bytes(src)) {
            Ok(bytes) => bytes,
            Err(error) => panic!("{error}"),
        };
        match image::load_from_memory(&softened) {
            Ok(img) => img.to_rgba8(),
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn left_and_bottom_edges_darker_than_center() {
        let out = soften_rgba(RgbaImage::from_pixel(48, 48, Rgba([255, 255, 255, 255])));
        let highlight = out.get_pixel(44, 4).0;
        let left = out.get_pixel(2, 24).0;
        let bottom = out.get_pixel(24, 45).0;

        assert!(
            luma(left) < luma(highlight),
            "left {left:?} should be darker than top-right {highlight:?}"
        );

        assert!(
            luma(bottom) < luma(highlight),
            "bottom {bottom:?} should be darker than top-right {highlight:?}"
        );
    }

    #[test]
    fn center_is_slightly_muted_and_edges_keep_accent_hue() {
        let out = soften_rgba(RgbaImage::from_pixel(48, 48, Rgba([220, 36, 48, 255])));
        let center = out.get_pixel(40, 8).0;
        let left = out.get_pixel(2, 24).0;

        assert!(
            center[0] < 220 && center[0] > 120,
            "center should be slightly muted, got {center:?}"
        );

        assert!(
            i16::from(center[0]) - i16::from(center[1]) > 40,
            "center should stay red-tinted, got {center:?}"
        );

        assert!(
            luma(left) < luma(center),
            "left edge {left:?} should be a stronger veil than center {center:?}"
        );
        
        assert!(
            i16::from(left[0]) - i16::from(left[1]) > 20,
            "edge veil should keep the image accent, got {left:?}"
        );
    }
}
