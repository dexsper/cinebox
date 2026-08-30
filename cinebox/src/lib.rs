//! Cinebox desktop application (egui / eframe glow).

#![forbid(unsafe_code)]

mod app;
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

use cinebox_core::i18n::Msg;

/// Run the desktop shell.
///
/// # Errors
///
/// Returns an [`eframe::Error`] if the window or renderer fails to start.
pub fn run() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_decorations(false)
            .with_title(Msg::AppTitle.en()),
        ..Default::default()
    };
    eframe::run_native(
        Msg::AppTitle.en(),
        native_options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
