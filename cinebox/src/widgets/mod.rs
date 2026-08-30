pub mod backdrop;
pub mod chrome;
pub mod intro;
pub mod poster;
pub mod scroll;
pub mod skeleton;

use egui::Ui;

use crate::theme::Theme;

const PAGE_SPINNER: f32 = 56.0;

/// Full-page loading spinner, centered, no caption.
pub fn page_spinner(ui: &mut Ui, theme: &Theme) {
    ui.centered_and_justified(|ui| {
        ui.add(egui::Spinner::new().size(PAGE_SPINNER).color(theme.muted));
    });
}
