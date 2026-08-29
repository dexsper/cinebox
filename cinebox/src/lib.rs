//! Cinebox Iced application.

#![forbid(unsafe_code)]

mod app;
mod images;
mod loaders;
mod message;
mod nav;
mod ui;

use iced::Size;

/// Run the desktop shell.
///
/// # Errors
///
/// Returns an [`iced::Error`] if the window or renderer fails to start.
pub fn run() -> iced::Result {
    iced::application(app::App::boot, app::App::update, app::App::view)
        .title(app::App::title)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .window_size(Size::new(1280.0, 800.0))
        .centered()
        .run()
}
