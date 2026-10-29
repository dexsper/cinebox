//! Small anchored popup, the flyout analog of [`super::drawer`] (full-height)
//! and `egui::Modal` (centered). Used by the player's Settings, Playlist, and
//! Volume controls: video stays visible around it.

use egui::{Align2, Area, Frame, Id, Margin, Order, Pos2, Rect, Stroke, Ui, pos2};

use crate::theme::Theme;
use crate::widgets::focus;

const ANCHOR_GAP: f32 = 10.0;

/// Default padding between the frame edge and the content.
pub(crate) const PAD: i8 = 10;

pub(crate) struct FlyoutOut {
    /// Painted popup rect (for hover checks).
    pub rect: Rect,

    /// A press landed outside both the popup and its anchor.
    pub dismissed: bool,
}

/// Which popup it is, what it hangs off, and how wide its content is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placement {
    pub id: &'static str,
    pub anchor: Rect,
    pub width: f32,
}

/// Paint an anchored popup just above `anchor`, right edges aligned.
///
/// Dismissal on Escape is the caller's job (the player consumes Escape before
/// navigation does); this only reports outside presses.
pub(crate) fn show(
    ctx: &egui::Context,
    theme: &Theme,
    placement: Placement,
    content: impl FnOnce(&mut Ui, &Theme),
) -> FlyoutOut {
    let anchor = placement.anchor;
    let pos = pos2(anchor.right(), anchor.top() - ANCHOR_GAP);

    show_at(ctx, theme, placement, (Align2::RIGHT_BOTTOM, pos), content)
}

/// Paint an anchored popup just below `anchor`, left edges aligned.
pub(crate) fn show_below(
    ctx: &egui::Context,
    theme: &Theme,
    placement: Placement,
    content: impl FnOnce(&mut Ui, &Theme),
) -> FlyoutOut {
    let anchor = placement.anchor;
    let pos = pos2(anchor.left(), anchor.bottom() + ANCHOR_GAP);

    show_at(ctx, theme, placement, (Align2::LEFT_TOP, pos), content)
}

/// `pin` is the corner of the popup that sits on the point.
fn show_at(
    ctx: &egui::Context,
    theme: &Theme,
    placement: Placement,
    pin: (Align2, Pos2),
    content: impl FnOnce(&mut Ui, &Theme),
) -> FlyoutOut {
    let Placement { id, anchor, width } = placement;
    let (align, pos) = pin;

    let response = Area::new(Id::new(id))
        .order(Order::Foreground)
        .pivot(align)
        .fixed_pos(pos)
        .constrain_to(ctx.content_rect().shrink(8.0))
        .show(ctx, |ui| {
            focus::trap(ui);
            Frame::new()
                .fill(theme.panel_elevated)
                .stroke(Stroke::new(1.0, theme.window_edge))
                .corner_radius(theme.rounding(theme.radius_dialog))
                .inner_margin(Margin::same(PAD))
                .show(ui, |ui| {
                    ui.set_width(width);
                    content(ui, theme);
                });
        })
        .response;

    let rect = response.rect;
    let dismissed = pressed_outside(ctx, rect, anchor);

    FlyoutOut { rect, dismissed }
}

pub(crate) fn pressed_outside(ctx: &egui::Context, rect: Rect, anchor: Rect) -> bool {
    let pressed = ctx.input(|i| i.pointer.any_pressed());
    if !pressed {
        return false;
    }

    let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) else {
        return false;
    };

    !rect.contains(pos) && !anchor.contains(pos)
}
