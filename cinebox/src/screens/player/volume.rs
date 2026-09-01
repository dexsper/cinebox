//! Volume flyout: vertical slider bound to the global player volume.

use egui::{Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::theme::Theme;

/// Content width passed to the flyout so it hugs the slider.
pub const WIDTH: f32 = 28.0;

const SLIDER_LEN: f32 = 110.0;
const TRACK_W: f32 = 6.0;
const HIT_W: f32 = 24.0;
const THUMB_R: f32 = 6.0;

/// Vertical 0–100 slider plus the numeric readout. Returns `true` on change.
pub fn slider(ui: &mut Ui, theme: &Theme, volume: &mut f64) -> bool {
    let mut changed = false;

    ui.vertical_centered(|ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(HIT_W, SLIDER_LEN), Sense::click_and_drag());
        let response = crate::widgets::button::pointing(response);

        let scrubbing = response.clicked() || response.dragged();

        if scrubbing && let Some(pos) = response.interact_pointer_pos() {
            let fraction = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);
            *volume = f64::from(fraction) * 100.0;
            changed = true;
        }

        paint(ui, theme, rect, *volume, response.hovered() || scrubbing);

        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("{:.0}", *volume))
                .size(theme.text_caption)
                .color(theme.muted_bright),
        );
    });

    changed
}

fn paint(ui: &Ui, theme: &Theme, rect: Rect, volume: f64, engaged: bool) {
    let track = Rect::from_center_size(rect.center(), vec2(TRACK_W, rect.height()));
    ui.painter().rect(
        track,
        3.0,
        theme.input_bg,
        Stroke::new(1.0, theme.window_edge),
        StrokeKind::Inside,
    );

    let fraction = (volume / 100.0).clamp(0.0, 1.0) as f32;
    let mut fill = track;
    fill.min.y = track.bottom() - track.height() * fraction;
    ui.painter().rect_filled(fill, 3.0, theme.selection);

    if !engaged {
        return;
    }

    let thumb = pos2(track.center().x, fill.top());
    ui.painter().circle_filled(thumb, THUMB_R, theme.selection);
    ui.painter()
        .circle_stroke(thumb, THUMB_R, Stroke::new(1.0, theme.title));
}
