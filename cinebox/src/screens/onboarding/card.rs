//! The wizard's centered card over a dimmed window, and its selectable rows.

use egui::{
    Align2, Area, Context, FontId, Frame, Id, Margin, Order, Rect, Sense, Stroke, Ui,
    UiBuilder, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::platform;
use crate::theme::Theme;
use crate::widgets::{button, focus};

const CARD_W: f32 = 560.0;
const WINDOW_GAP: f32 = 24.0;
const SLIDE: f32 = 12.0;
const CHOICE_H: f32 = 44.0;
const CHOICE_WITH_NOTE_H: f32 = 56.0;
const CHOICE_PAD_X: f32 = 14.0;

/// Paint `add` in a card centered below the title bar (on TV, on the whole
/// screen), dimming and blocking everything else. `t` (0..=1) drives the fade
/// and slide-in.
pub fn show(ctx: &Context, theme: &Theme, t: f32, add: impl FnOnce(&mut Ui)) {
    let body = body_rect(ctx, theme);
    let height_id = Id::new("cinebox-onboarding-card-h");
    let remembered = ctx.data(|d| d.get_temp::<f32>(height_id));
    let last_h = remembered.unwrap_or(body.height());

    Area::new(Id::new("cinebox-onboarding"))
        .order(Order::Foreground)
        .fixed_pos(body.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_min_size(body.size());
            ui.set_clip_rect(body);
            ui.painter().rect_filled(body, 0.0, theme.overlay_at(t));
            ui.interact(body, Id::new("cinebox-onboarding-block"), Sense::CLICK);
            focus::trap(ui);

            let card = card_rect(body, last_h, t);
            let builder = UiBuilder::new().max_rect(card);
            let scope = ui.scope_builder(builder, |ui| card_frame(ui, theme, add));

            let measured = scope.response.rect.height();
            ctx.data_mut(|d| d.insert_temp(height_id, measured));
        });
}

/// Height the card body may scroll within, leaving room for header and footer.
#[must_use]
pub fn max_body_height(ctx: &Context, theme: &Theme, chrome_h: f32) -> f32 {
    let body = body_rect(ctx, theme);
    (body.height() - WINDOW_GAP * 2.0 - chrome_h).max(160.0)
}

fn body_rect(ctx: &Context, theme: &Theme) -> Rect {
    let full = ctx.content_rect();
    // The desktop title bar stays usable; a TV has no window to move or close.
    if platform::is_tv(ctx) {
        return full;
    }

    let top = full.top() + theme.title_bar_h;
    Rect::from_min_max(pos2(full.left(), top), full.right_bottom())
}

fn card_rect(body: Rect, height: f32, t: f32) -> Rect {
    let width = CARD_W.min(body.width() - WINDOW_GAP * 2.0);
    let height = height.min(body.height() - WINDOW_GAP * 2.0);
    let slide = (1.0 - t) * SLIDE;

    let left = body.center().x - width / 2.0;
    let top = body.center().y - height / 2.0 + slide;
    let bottom = body.bottom() - WINDOW_GAP;

    Rect::from_min_max(pos2(left, top), pos2(left + width, bottom))
}

fn card_frame(ui: &mut Ui, theme: &Theme, add: impl FnOnce(&mut Ui)) {
    let radius = theme.rounding(theme.radius_dialog);
    let frame = Frame::new()
        .fill(theme.panel_elevated)
        .stroke(Stroke::new(1.0, theme.window_edge))
        .corner_radius(radius)
        .inner_margin(Margin::same(28));

    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}

/// A full-width selectable row: a title and an optional muted note under it.
pub fn choice(ui: &mut Ui, theme: &Theme, title: &str, note: Option<&str>, selected: bool) -> bool {
    let size = vec2(ui.available_width(), choice_height(note));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = button::pointing(response);
    if selected {
        focus::prefer(&response);
    }

    let enabled = response.enabled();
    let kind = WidgetType::SelectableLabel;
    response.widget_info(|| WidgetInfo::selected(kind, enabled, selected, title));

    let fill = choice_fill(theme, selected, focus::lit(&response));
    let stroke = choice_stroke(theme, selected);
    let radius = theme.rounding(theme.radius_card);
    let painter = ui.painter();
    painter.rect(rect, radius, fill, stroke, egui::StrokeKind::Inside);

    let left = rect.left() + CHOICE_PAD_X;
    let Some(note) = note else {
        let font = FontId::proportional(theme.text_body);
        let at = pos2(left, rect.center().y);
        painter.text(at, Align2::LEFT_CENTER, title, font, theme.title);
        return response.clicked();
    };

    let title_font = FontId::proportional(theme.text_body);
    let note_font = FontId::proportional(theme.text_small);

    let upper = pos2(left, rect.center().y - 1.0);
    let lower = pos2(left, rect.center().y + 1.0);

    let (title_color, note_color) = (theme.title, theme.muted);
    painter.text(upper, Align2::LEFT_BOTTOM, title, title_font, title_color);
    painter.text(lower, Align2::LEFT_TOP, note, note_font, note_color);

    response.clicked()
}

fn choice_height(note: Option<&str>) -> f32 {
    if note.is_some() {
        return CHOICE_WITH_NOTE_H;
    }

    CHOICE_H
}

fn choice_fill(theme: &Theme, selected: bool, hovered: bool) -> egui::Color32 {
    if selected {
        return theme.card_selected;
    }

    if hovered {
        return theme.widget_hover;
    }

    theme.input_bg
}

fn choice_stroke(theme: &Theme, selected: bool) -> Stroke {
    if selected {
        return Stroke::new(1.5, theme.btn_primary_bg);
    }

    Stroke::new(1.0, theme.window_edge)
}
