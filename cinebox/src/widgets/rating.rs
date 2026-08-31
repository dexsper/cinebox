//! Shared "rating pill" row: TMDB score plus an optional certification badge.
//!
//! Both the media details page and the torrent explorer's left pane show ratings
//! for the same movie/show, so the pill styling lives here instead of being
//! duplicated per screen.

use egui::{Align, Frame, Layout, Margin, RichText, Ui};

use crate::theme::Theme;

const PILL_MARGIN: i8 = 12;
const PILL_GAP: f32 = 8.0;
const ROW_GAP: f32 = 10.0;

/// Draws the TMDB score pill and, if present, a certification pill next to it.
/// Draws nothing if there is neither a vote nor a certification to show.
pub fn row(ui: &mut Ui, theme: &Theme, vote: Option<f32>, cert: Option<&str>) {
    let vote = vote.filter(|v| *v > 0.0);
    let cert = cert.filter(|s| !s.is_empty());
    if vote.is_none() && cert.is_none() {
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
        
        if let Some(cert) = cert {
            pill(ui, theme, |ui| {
                ui.label(RichText::new(cert).size(theme.text_subtitle).color(theme.title));
            });
        }
    });
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
        .inner_margin(Margin::symmetric(PILL_MARGIN, 6))
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
