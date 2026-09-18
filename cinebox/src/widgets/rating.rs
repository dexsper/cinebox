//! Shared "rating pill" row: TMDB score plus optional status/certification badges.
//!
//! Both the media details page and the torrent explorer's left pane show ratings
//! for the same movie/show, so the pill styling lives here instead of being
//! duplicated per screen.

use egui::{Align, Frame, Layout, Margin, RichText, Ui};

use crate::theme::Theme;

const PILL_MARGIN_X: i8 = 12;
const PILL_MARGIN_Y: i8 = 6;
const PILL_GAP: f32 = 8.0;
const ROW_GAP: f32 = 10.0;

/// Draws the TMDB score pill, an optional release/air status pill (e.g.
/// "Released", "Ongoing"), and an optional certification pill, in that order.
/// Draws nothing if none of the three apply.
pub fn row(ui: &mut Ui, theme: &Theme, vote: Option<f32>, status: Option<&str>, cert: Option<&str>) {
    let vote = vote.filter(|v| *v > 0.0);
    let status = status.filter(|s| !s.is_empty());
    let cert = cert.filter(|s| !s.is_empty());
    if vote.is_none() && status.is_none() && cert.is_none() {
        return;
    }

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = ROW_GAP;
        if let Some(vote) = vote {
            pill(ui, theme, |ui| {
                let score = format!("{vote:.1}");
                ui.label(RichText::new(score).size(theme.text_subtitle).color(theme.rate));
                ui.label(RichText::new("TMDB").size(theme.text_caption).color(theme.muted));
            });
        }

        if let Some(status) = status {
            pill(ui, theme, |ui| {
                ui.label(RichText::new(status).size(theme.text_subtitle).color(theme.title));
            });
        }

        if let Some(cert) = cert {
            pill(ui, theme, |ui| {
                ui.label(RichText::new(cert).size(theme.text_subtitle).color(theme.title));
            });
        }
    });
}

/// Total pill-row height (frame plus margins), so callers can reserve space
/// for it before drawing it, e.g. to center it inside a flexible gap.
#[must_use]
pub fn row_height(ui: &Ui, theme: &Theme) -> f32 {
    inner_h(ui, theme) + f32::from(PILL_MARGIN_Y) * 2.0
}

/// Pill content height, measured from the real `text_subtitle` metrics (the largest
/// text drawn inside) instead of a hardcoded guess that can drift from the real font.
fn inner_h(ui: &Ui, theme: &Theme) -> f32 {
    ui.ctx()
        .fonts_mut(|f| f.row_height(&theme.ui_font(theme.text_subtitle)))
}

fn pill(ui: &mut Ui, theme: &Theme, add: impl FnOnce(&mut Ui)) {
    let inner_h = inner_h(ui, theme);

    Frame::new()
        .fill(theme.rating_pill)
        .corner_radius(6)
        .inner_margin(Margin::symmetric(PILL_MARGIN_X, PILL_MARGIN_Y))
        .show(ui, |ui| {
            ui.set_min_height(inner_h);
            ui.set_max_height(inner_h);
            ui.spacing_mut().item_spacing.x = PILL_GAP;
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.set_min_height(inner_h);
                ui.set_max_height(inner_h);
                add(ui);
            });
        });
}
