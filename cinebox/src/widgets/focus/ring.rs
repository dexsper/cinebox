//! The ring around the focused widget, the one focus mark a remote user looks for.

use egui::{Context, CornerRadius, Stroke, StrokeKind};

use super::State;
use crate::theme::Theme;

pub(super) fn paint(ctx: &Context, theme: &Theme, state: &State) {
    let Some(id) = ctx.memory(|mem| mem.focused()) else {
        return;
    };

    if state.own_marks.contains(&id) {
        return;
    }

    let Some(response) = ctx.read_response(id) else {
        return;
    };

    let field = state.field_rings.iter().find(|(edit, _)| *edit == id);
    let (target, visible) = match field {
        Some((_, field)) => (*field, *field),
        None => (response.rect, response.interact_rect),
    };

    // Clipped away entirely, e.g. scrolled out of its list.
    if visible.width() < 1.0 || visible.height() < 1.0 {
        return;
    }

    let pad = theme.ring_w + theme.ring_gap;
    let ring = target.expand(theme.ring_gap);
    let radius = CornerRadius::same(theme.radius_card.round() as u8);
    ctx.layer_painter(response.layer_id)
        .with_clip_rect(visible.expand(pad))
        .rect_stroke(
            ring,
            radius,
            Stroke::new(theme.ring_w, theme.ring),
            StrokeKind::Outside,
        );
}
