//! Cinebox application (egui / eframe glow) for desktop and Android TV.

#![forbid(unsafe_code)]

rust_i18n::i18n!("locales", fallback = "en");

mod app;
mod i18n;
pub mod platform;
mod screens;
mod services;
mod theme;
mod widgets;

#[cfg(test)]
mod ui_tests;

use rust_i18n::t;

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
    let creator = app_creator(platform::Host::desktop());
    eframe::run_native(&app_name(), native_options(), creator)
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

/// What eframe identifies the app by (window title, storage).
#[must_use]
pub fn app_name() -> String {
    t!("app.title").into_owned()
}

/// The app for an entry point that runs eframe itself.
pub fn app_creator(host: platform::Host) -> eframe::AppCreator<'static> {
    Box::new(move |cc| Ok(Box::new(app::App::new(cc, host))))
}

fn app_icon() -> egui::IconData {
    egui::IconData {
        rgba: include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.rgba")).to_vec(),
        width: ICON_PX,
        height: ICON_PX,
    }
}
