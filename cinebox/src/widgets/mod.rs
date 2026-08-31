pub mod backdrop;
pub mod button;
pub mod chrome;
pub mod combo;
pub mod drawer;
pub mod intro;
pub mod poster;
pub mod rating;
pub mod scroll;
pub mod skeleton;

use cinebox_core::i18n::Msg;
use egui::{Align, Color32, Direction, Layout, RichText, Sense, Ui, UiBuilder, Vec2, vec2};
use egui_material_icons::icons::ICON_REFRESH;

use crate::theme::Theme;

use self::button::Opts;

const PAGE_SPINNER: f32 = 56.0;
const PAGE_COPY_MAX_W: f32 = 480.0;
const PAGE_BODY_FALLBACK_H: f32 = 96.0;

/// Claim the leftover clip rect so status UI can sit in the middle of a list pane.
fn fill_remaining(ui: &mut Ui) -> Option<egui::Rect> {
    let rect = ui.available_rect_before_wrap().intersect(ui.clip_rect());
    if rect.height() < 8.0 {
        return None;
    }

    ui.advance_cursor_after_rect(rect);
    Some(rect)
}

/// Full-page loading spinner, centered, no caption.
pub fn page_spinner(ui: &mut Ui, theme: &Theme) {
    let Some(rect) = fill_remaining(ui) else {
        return;
    };

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::centered_and_justified(Direction::TopDown)),
        |ui| {
            ui.add(egui::Spinner::new().size(PAGE_SPINNER).color(theme.muted));
        },
    );
}

/// Centered status copy (errors, empty lists).
pub fn page_message(ui: &mut Ui, theme: &Theme, text: &str, color: Color32) {
    in_remaining(ui, |ui| {
        ui.label(
            RichText::new(text)
                .font(theme.title_font(theme.text_display))
                .color(color),
        );
    });
}

/// Centered error plus Retry. Paints in the remaining clip rect so clicks hit the button.
pub fn page_error(ui: &mut Ui, theme: &Theme, text: &str) -> bool {
    in_remaining(ui, |ui| error_body(ui, theme, text)).unwrap_or(false)
}

fn in_remaining<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let rect = fill_remaining(ui)?;
    let id = ui.id().with("page-body");
    let mut out = None;

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::centered_and_justified(Direction::TopDown)),
        |ui| {
            let slot = body_slot(ui, id, rect.width());
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(slot)
                    .layout(Layout::top_down(Align::Center)),
                |ui| {
                    ui.set_width(slot.width());
                    out = Some(add(ui));
                    remember_body_size(ui, id);
                },
            );
        },
    );

    out
}

fn body_slot(ui: &mut Ui, id: egui::Id, pane_w: f32) -> egui::Rect {
    let width = PAGE_COPY_MAX_W.min(pane_w);
    let last = ui.ctx().data(|data| data.get_temp::<Vec2>(id));
    let size = last.unwrap_or(vec2(width, PAGE_BODY_FALLBACK_H));
    let (slot, _) = ui.allocate_exact_size(size, Sense::hover());

    slot
}

fn remember_body_size(ui: &Ui, id: egui::Id) {
    let measured = ui.min_size();
    let last = ui.ctx().data(|data| data.get_temp::<Vec2>(id));
    let same = last.is_some_and(|prev| (prev - measured).length() < 0.5);
    if same {
        return;
    }

    ui.ctx().data_mut(|data| data.insert_temp(id, measured));
    ui.ctx().request_repaint();
}

fn error_body(ui: &mut Ui, theme: &Theme, text: &str) -> bool {
    ui.label(
        RichText::new(text)
            .font(theme.emphasis_font(theme.text_display))
            .color(theme.err),
    );
    ui.add_space(16.0);
    let retry_size = vec2(128.0, combo::HEIGHT);

    button::icon_label(
        ui,
        theme,
        ICON_REFRESH,
        Msg::Retry.en(),
        Opts::secondary(retry_size),
    )
}
