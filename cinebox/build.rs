//! Rasterize `assets/icon.svg` for the window icon and the Windows .exe resource.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ico::{IconDir, IconDirEntry, IconImage, ResourceType};
use resvg::tiny_skia::{Pixmap, PremultipliedColorU8, Transform};
use resvg::usvg;

const WINDOW_PX: u32 = 256;
const ICO_SIZES: [u32; 4] = [16, 32, 48, 256];

fn main() {
    if let Err(error) = build() {
        panic!("{error:#}");
    }
}

fn build() -> Result<()> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let svg_path = manifest.join("assets/icon.svg");
    println!("cargo:rerun-if-changed={}", svg_path.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let svg = fs::read(&svg_path).with_context(|| format!("read {}", svg_path.display()))?;
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).context("parse icon.svg")?;

    let rgba = rasterize(&tree, WINDOW_PX)?;
    let rgba_path = out_dir.join("icon-256.rgba");
    fs::write(&rgba_path, &rgba).with_context(|| format!("write {}", rgba_path.display()))?;

    let ico_path = out_dir.join("icon.ico");
    write_ico(&tree, &ico_path)?;

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        let ico = ico_path
            .to_str()
            .context("icon.ico path is not valid UTF-8")?;

        resource.set_icon(ico);
        resource.compile().context("embed Windows icon resource")?;
    }

    Ok(())
}

fn write_ico(tree: &usvg::Tree, path: &Path) -> Result<()> {
    let mut dir = IconDir::new(ResourceType::Icon);
    for size in ICO_SIZES {
        let rgba = rasterize(tree, size)?;
        let image = IconImage::from_rgba_data(size, size, rgba);
        dir.add_entry(IconDirEntry::encode(&image)?);
    }

    let mut file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    dir.write(&mut file)?;
    file.flush()?;

    Ok(())
}

fn rasterize(tree: &usvg::Tree, size: u32) -> Result<Vec<u8>> {
    let svg_w = tree.size().width();
    if svg_w <= 0.0 {
        bail!("icon.svg has no width");
    }

    let Some(mut pixmap) = Pixmap::new(size, size) else {
        bail!("could not allocate {size}x{size} pixmap");
    };

    let scale = size as f32 / svg_w;
    let transform = Transform::from_scale(scale, scale);
    resvg::render(tree, transform, &mut pixmap.as_mut());

    Ok(unpremultiply(pixmap.pixels()))
}

fn unpremultiply(pixels: &[PremultipliedColorU8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(pixels.len().saturating_mul(4));
    for pixel in pixels {
        let alpha = u16::from(pixel.alpha());
        if alpha == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }

        if alpha == 255 {
            rgba.extend_from_slice(&[pixel.red(), pixel.green(), pixel.blue(), 255]);
            continue;
        }

        let red = (u16::from(pixel.red()) * 255 / alpha) as u8;
        let green = (u16::from(pixel.green()) * 255 / alpha) as u8;
        let blue = (u16::from(pixel.blue()) * 255 / alpha) as u8;
        rgba.extend_from_slice(&[red, green, blue, alpha as u8]);
    }

    rgba
}
