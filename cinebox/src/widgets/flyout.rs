//! Small anchored popup, the flyout analog of [`super::drawer`] (full-height)
//! and `egui::Modal` (centered). Used by the player's Settings, Playlist, and
//! Volume controls: video stays visible around it.

use egui::{Align2, Area, Frame, Id, Margin, Order, Pos2, Rect, Stroke, Ui, pos2};

use crate::theme::Theme;

const ANCHOR_GAP: f32 = 10.0;

/// Default padding between the frame edge and the content.
pub const PAD: i8 = 10;

pub struct FlyoutOut {
    /// Painted popup rect (for hover checks).
    pub rect: Rect,
    /// A press landed outside both the popup and its anchor.
    pub dismissed: bool,
}

/// Paint an anchored popup just above `anchor`, right edges aligned.
///
/// Dismissal on Escape is the caller's job (the player consumes Escape before
/// navigation does); this only reports outside presses.
pub fn show(
    ctx: &egui::Context,
    id: &'static str,
    anchor: Rect,
    theme: &Theme,
    width: f32,
    margin: Margin,
    content: impl FnOnce(&mut Ui, &Theme),
) -> FlyoutOut {
    let spec = Spec {
        id,
        anchor,
        theme,
        width,
        margin,
        align: Align2::RIGHT_BOTTOM,
        pos: pos2(anchor.right(), anchor.top() - ANCHOR_GAP),
    };

    show_at(ctx, spec, content)
}

struct Spec<'a> {
    id: &'static str,
    anchor: Rect,
    theme: &'a Theme,
    width: f32,
    margin: Margin,
    align: Align2,
    pos: Pos2,
}

fn show_at(ctx: &egui::Context, spec: Spec<'_>, content: impl FnOnce(&mut Ui, &Theme)) -> FlyoutOut {
    let Spec {
        id,
        anchor,
        theme,
        width,
        margin,
        align,
        pos,
    } = spec;

    let response = Area::new(Id::new(id))
        .order(Order::Foreground)
        .pivot(align)
        .fixed_pos(pos)
        .constrain_to(ctx.content_rect().shrink(8.0))
        .show(ctx, |ui| {
            Frame::new()
                .fill(theme.panel_elevated)
                .stroke(Stroke::new(1.0, theme.window_edge))
                .corner_radius(theme.rounding(theme.radius_dialog))
                .inner_margin(margin)
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
