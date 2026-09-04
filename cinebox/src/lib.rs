//! Cinebox desktop application (egui / eframe glow).

#![forbid(unsafe_code)]

rust_i18n::i18n!("locales", fallback = "en");

mod app;
mod fonts;
mod i18n;
mod images;
mod jobs;
mod nav;
mod screens;
mod services;
mod theme;
mod toasts;
mod widgets;

#[cfg(test)]
mod ui_tests;

use rust_i18n::t;

const ICON_PX: u32 = 256;

/// Run the desktop shell.
///
/// # Errors
///
/// Returns an [`eframe::Error`] if the window or renderer fails to start.
pub fn run() -> eframe::Result {
    let title = t!("app.title");
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_decorations(false)
            .with_title(title.as_ref())
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        title.as_ref(),
        native_options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

fn app_icon() -> egui::IconData {
    egui::IconData {
        rgba: include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.rgba")).to_vec(),
        width: ICON_PX,
        height: ICON_PX,
    }
}
