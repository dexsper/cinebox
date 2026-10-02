//! Cinebox application (egui / eframe glow) for desktop and Android TV.

#![forbid(unsafe_code)]

rust_i18n::i18n!("locales", fallback = "en");

mod app;
mod discovery;
mod errors;
mod fonts;
mod i18n;
mod images;
mod jobs;
mod library;
mod nav;
mod platform;
mod screens;
mod services;
mod settings_input;
mod theme;
mod toasts;
mod widgets;

#[cfg(test)]
mod ui_tests;

use rust_i18n::t;

pub use platform::{Form, Host, KeepAwake};

const ICON_PX: u32 = 256;

/// Wayland `app_id` / X11 `WM_CLASS`; must match the `.desktop` file name so the
/// desktop shows the right name and icon for the window.
const APP_ID: &str = "io.github.dexsper.cinebox";

/// Run the desktop shell.
///
/// # Errors
///
/// Returns an [`eframe::Error`] if the window or renderer fails to start.
pub fn run() -> eframe::Result {
    run_with(native_options(), Host::default())
}

/// Window setup shared by every platform; Android adds its `android_app`.
#[must_use]
pub fn native_options() -> eframe::NativeOptions {
    let title = t!("app.title");

    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_decorations(false)
            .with_title(title.as_ref())
            .with_app_id(APP_ID)
            .with_icon(app_icon()),
        ..Default::default()
    }
}

/// Run the app with prepared options.
///
/// # Errors
///
/// Returns an [`eframe::Error`] if the window or renderer fails to start.
pub fn run_with(native_options: eframe::NativeOptions, host: Host) -> eframe::Result {
    let title = t!("app.title");

    eframe::run_native(
        title.as_ref(),
        native_options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, host)))),
    )
}

fn app_icon() -> egui::IconData {
    egui::IconData {
        rgba: include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.rgba")).to_vec(),
        width: ICON_PX,
        height: ICON_PX,
    }
}
