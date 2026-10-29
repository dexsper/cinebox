//! Pulsing placeholder bars while card details load.

use egui::{Sense, Ui, Vec2};

use crate::theme::{self, Theme};

/// 0..1 sine pulse. Caller should `request_repaint` once per frame.
#[must_use]
pub(crate) fn pulse(ui: &Ui) -> f32 {
    let time = ui.input(|i| i.time);
    theme::narrow((time * 2.8).sin()).mul_add(0.5, 0.5)
}

pub(crate) fn bar(ui: &mut Ui, theme: &Theme, width: f32, height: f32, pulse: f32) {
    let fill = theme.poster_placeholder.gamma_multiply(0.55 + 0.45 * pulse);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter().rect_filled(rect, theme.rounding(theme.radius_card), fill);
}

pub(crate) fn poster(ui: &mut Ui, theme: &Theme, size: Vec2, pulse: f32) {
    let fill = theme.poster_placeholder.gamma_multiply(0.55 + 0.45 * pulse);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, theme.rounding(theme.radius_poster), fill);
}
