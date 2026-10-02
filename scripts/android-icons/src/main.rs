//! Rasterize `cinebox/assets/icon.svg` into `android/app/src/main/res`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;

/// Launcher icon edge per density bucket (48dp).
const LAUNCHER: [(&str, u32); 5] = [
    ("mdpi", 48),
    ("hdpi", 72),
    ("xhdpi", 96),
    ("xxhdpi", 144),
    ("xxxhdpi", 192),
];

/// Android TV banner: 160x90dp, shipped at xhdpi.
const BANNER_W: u32 = 320;
const BANNER_H: u32 = 180;

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let assets = root.join("cinebox/assets");
    let res = root.join("android/app/src/main/res");

    let icon = fs::read(assets.join("icon.svg")).context("read icon.svg")?;
    let options = options(&assets)?;
    let tree = usvg::Tree::from_data(&icon, &options).context("parse icon.svg")?;

    for (density, size) in LAUNCHER {
        let path = res.join(format!("mipmap-{density}/ic_launcher.png"));
        save(&render(&tree, size, size)?, &path)?;
    }

    let banner = banner_svg();
    let tree = usvg::Tree::from_str(&banner, &options).context("parse banner")?;
    save(
        &render(&tree, BANNER_W, BANNER_H)?,
        &res.join("drawable-xhdpi/banner.png"),
    )
}

/// `href="icon.svg"` and the Plex font resolve against the app assets.
fn options(assets: &Path) -> Result<usvg::Options<'static>> {
    let mut options = usvg::Options {
        resources_dir: Some(assets.to_path_buf()),
        ..usvg::Options::default()
    };

    let font = assets.join("fonts/IBMPlexSans-Medium.ttf");
    options
        .fontdb_mut()
        .load_font_file(&font)
        .with_context(|| format!("load {}", font.display()))?;

    Ok(options)
}

fn banner_svg() -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{BANNER_W}" height="{BANNER_H}">
  <rect width="{BANNER_W}" height="{BANNER_H}" fill="#0e0e15"/>
  <image x="28" y="46" width="88" height="88" href="icon.svg"/>
  <text x="132" y="104" font-family="IBM Plex Sans" font-weight="500" font-size="40"
        fill="#f5f5f7">Cinebox</text>
</svg>"##
    )
}

fn render(tree: &usvg::Tree, width: u32, height: u32) -> Result<Pixmap> {
    let size = tree.size();
    if size.width() <= 0.0 {
        bail!("svg has no width");
    }

    let Some(mut pixmap) = Pixmap::new(width, height) else {
        bail!("could not allocate {width}x{height} pixmap");
    };

    let scale_x = width as f32 / size.width();
    let scale_y = height as f32 / size.height();
    resvg::render(
        tree,
        Transform::from_scale(scale_x, scale_y),
        &mut pixmap.as_mut(),
    );

    Ok(pixmap)
}

fn save(pixmap: &Pixmap, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }

    pixmap
        .save_png(path)
        .with_context(|| format!("write {}", path.display()))
}
