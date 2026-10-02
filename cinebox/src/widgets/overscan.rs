//! Fitting the interface to a TV that crops the picture's edges: marks in the
//! corners of what stays visible, and a stepper for how much is cropped.

use cinebox_core::Overscan;
use egui::{
    Align2, Context, FontId, Id, Key, LayerId, Order, PointerButton, Pos2, Rect, Response, Sense,
    Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, pos2, vec2,
};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT};

use crate::platform::{self, Direction, UiSound};
use crate::theme::Theme;
use crate::widgets::{button, focus};

const STEPPER_H: f32 = 44.0;
const CHEVRON_W: f32 = 48.0;
const MARK_LEN: f32 = 32.0;
const MARK_W: f32 = 3.0;
const CENTER: Align2 = Align2::CENTER_CENTER;

#[derive(Clone, Copy)]
enum Change {
    Less,
    More,
}

impl Change {
    fn apply(self, overscan: Overscan) -> Overscan {
        match self {
            Self::Less => overscan.shrink(),
            Self::More => overscan.grow(),
        }
    }

    fn direction(self) -> Direction {
        match self {
            Self::Less => Direction::Left,
            Self::More => Direction::Right,
        }
    }
}

/// An L in each corner of the area the controls keep to. Calibrated right,
/// all four are fully visible and touch the screen's edges.
pub fn corner_marks(ctx: &Context, theme: &Theme) {
    let safe = ctx.content_rect() - platform::edge_inset(ctx);
    let layer = LayerId::new(Order::Tooltip, Id::new("cinebox-overscan-marks"));
    let painter = ctx.layer_painter(layer);
    let stroke = Stroke::new(MARK_W, theme.title);

    let corners = [
        (safe.left_top(), vec2(1.0, 1.0)),
        (safe.right_top(), vec2(-1.0, 1.0)),
        (safe.left_bottom(), vec2(1.0, -1.0)),
        (safe.right_bottom(), vec2(-1.0, -1.0)),
    ];

    for (corner, inward) in corners {
        let tip = corner + inward * (MARK_W * 0.5);
        let along_x = tip + vec2(inward.x * MARK_LEN, 0.0);
        let along_y = tip + vec2(0.0, inward.y * MARK_LEN);
        painter.line_segment([tip, along_x], stroke);
        painter.line_segment([tip, along_y], stroke);
    }
}

/// One focus stop: Left and Right on the remote crop less or more; with a
/// pointer the chevrons do.
pub fn stepper(ui: &mut Ui, theme: &Theme, overscan: &mut Overscan, label: &str) -> Response {
    let size = vec2(ui.available_width(), STEPPER_H);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let mut response = button::pointing(response);
    let spoken = format!("{label} {}", percent(*overscan));
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, &spoken));

    let before = *overscan;
    if let Some(change) = requested(ui, &response, rect) {
        *overscan = change.apply(*overscan);
        if *overscan != before {
            response.mark_changed();
            on_changed(ui, change);
        }
    }

    paint(ui, theme, &response, rect, *overscan);

    response
}

fn requested(ui: &Ui, response: &Response, rect: Rect) -> Option<Change> {
    let remote = response.has_focus() && platform::profile(ui.ctx()).is_directional();
    if remote {
        focus::hold_arrows(ui, response.id, true, false);
        return arrow_change(ui);
    }

    if !response.clicked_by(PointerButton::Primary) {
        return None;
    }

    let pos = response.interact_pointer_pos()?;
    let less = Rect::from_min_size(rect.min, vec2(CHEVRON_W, rect.height()));
    if less.contains(pos) {
        return Some(Change::Less);
    }

    let more = Rect::from_min_max(pos2(rect.right() - CHEVRON_W, rect.top()), rect.max);
    more.contains(pos).then_some(Change::More)
}

fn arrow_change(ui: &Ui) -> Option<Change> {
    if ui.input(|i| i.key_pressed(Key::ArrowLeft)) {
        return Some(Change::Less);
    }

    if ui.input(|i| i.key_pressed(Key::ArrowRight)) {
        return Some(Change::More);
    }

    None
}

/// The edge margin is read at the start of a frame, so the next one shows it.
fn on_changed(ui: &Ui, change: Change) {
    let sound = UiSound::Navigate {
        direction: change.direction(),
        repeat: false,
    };
    platform::device(ui.ctx()).play_sound(sound);
    ui.ctx().request_repaint();
}

fn paint(ui: &Ui, theme: &Theme, response: &Response, rect: Rect, overscan: Overscan) {
    let fill = if focus::lit(response) {
        theme.widget_hover
    } else {
        theme.input_bg
    };

    let painter = ui.painter();
    let radius = theme.rounding(theme.radius_card);
    let edge = Stroke::new(1.0, theme.window_edge);
    painter.rect(rect, radius, fill, edge, StrokeKind::Inside);

    let font = theme.ui_font(theme.text_label);
    let value = percent(overscan);
    painter.text(rect.center(), CENTER, value, font, theme.title);

    let left = pos2(rect.left() + CHEVRON_W * 0.5, rect.center().y);
    let right = pos2(rect.right() - CHEVRON_W * 0.5, rect.center().y);
    chevron(ui, theme, ICON_CHEVRON_LEFT, left, overscan.can_shrink());
    chevron(ui, theme, ICON_CHEVRON_RIGHT, right, overscan.can_grow());
}

fn chevron(ui: &Ui, theme: &Theme, icon: MaterialIcon, at: Pos2, enabled: bool) {
    let color = if enabled {
        theme.muted_bright
    } else {
        theme.muted.gamma_multiply(0.4)
    };

    let font = FontId::new(theme.text_icon_lg, icon.font_family());
    let painter = ui.painter();
    painter.text(at, CENTER, icon.codepoint, font, color);
}

/// "2%", "2.5%".
fn percent(overscan: Overscan) -> String {
    let permille = overscan.permille();
    let whole = permille / 10;
    let tenth = permille % 10;
    if tenth == 0 {
        return format!("{whole}%");
    }

    format!("{whole}.{tenth}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_drops_a_zero_tenth() {
        let two = Overscan::default();
        assert_eq!(percent(two), "2%");
        assert_eq!(percent(two.grow()), "2.5%");
        assert_eq!(percent(two.shrink().shrink().shrink().shrink()), "0%");
    }
}
