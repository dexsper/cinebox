//! Shared styled controls for the settings drawer.

use cinebox_core::SecretString;
use cinebox_core::i18n::Msg;
use egui::{
    Align, Atom, CornerRadius, CursorIcon, Frame, Layout, Margin, Rect, RichText, Sense, Stroke,
    TextEdit, Ui, UiBuilder, Vec2, pos2, vec2,
};
use egui_async::Bind;
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_ARROW_BACK, ICON_CHEVRON_RIGHT, ICON_DELETE_SWEEP, ICON_SPEED,
};

use super::catalog::Category;
use super::speed::{self, SpeedMeter};
use crate::jobs::JobError;
use crate::theme::Theme;

const CATEGORY_H: f32 = 72.0;
const ICON_WELL: f32 = 40.0;
const TOGGLE_W: f32 = 44.0;
const TOGGLE_H: f32 = 26.0;
const TOGGLE_GAP: f32 = 12.0;
const HINT_GAP: f32 = 3.0;
const ROW_PAD_Y: f32 = 4.0;
const INPUT_H: f32 = crate::widgets::button::CONTROL_H;
const ACTION_H: f32 = 36.0;

pub fn category_row(ui: &mut Ui, theme: &Theme, cat: &Category) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), CATEGORY_H), Sense::hover());
    if ui.rect_contains_pointer(rect) {
        ui.painter()
            .rect_filled(rect, theme.rounding(theme.radius_card), theme.widget_hover);
    }

    let inner = rect.shrink2(vec2(12.0, 10.0));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );

    row.style_mut().interaction.selectable_labels = false;
    icon_well(&mut row, theme, cat);

    let title = cat.title.t();
    let subtitle = cat.subtitle.t();

    row.add_space(12.0);
    row.vertical(|ui| {
        ui.label(
            RichText::new(title)
                .font(theme.title_font(theme.text_section))
                .color(theme.title),
        );
        ui.label(
            RichText::new(subtitle)
                .size(theme.text_small)
                .color(theme.muted),
        );
    });

    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(
            ICON_CHEVRON_RIGHT
                .rich_text()
                .size(theme.text_icon_lg)
                .color(theme.muted),
        );
    });

    // Last so it sits above labels and eats the click instead of text selection.
    hit_on_top(ui, rect, cat.title.en()).clicked()
}

fn icon_well(ui: &mut Ui, theme: &Theme, cat: &Category) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(ICON_WELL), Sense::hover());
    ui.painter()
        .rect_filled(rect, theme.rounding(theme.radius_card), theme.input_bg);

    ui.new_child(UiBuilder::new().max_rect(rect))
        .centered_and_justified(|ui| {
            ui.label(
                cat.icon
                    .rich_text()
                    .size(theme.text_icon_lg)
                    .color(theme.title),
            );
        });
}

pub fn nav_header(ui: &mut Ui, theme: &Theme, title: &str) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );

    let back = crate::widgets::button::add(
        &mut row,
        theme,
        ICON_ARROW_BACK
            .rich_text()
            .size(theme.text_icon_md)
            .color(theme.title),
        crate::widgets::button::Opts::secondary(vec2(32.0, 32.0)),
    )
    .on_hover_text(Msg::NavBack.t())
    .clicked();

    row.add_space(8.0);
    row.label(
        RichText::new(title)
            .font(theme.title_font(theme.text_heading))
            .color(theme.title),
    );

    back
}

pub fn drawer_title(ui: &mut Ui, theme: &Theme, title: &str) {
    ui.label(
        RichText::new(title)
            .font(theme.title_font(theme.text_display))
            .color(theme.title),
    );
}

pub fn error_line(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.label(RichText::new(text).size(theme.text_small).color(theme.err));
}

pub fn toggle_row(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    hint: Option<&str>,
    value: &mut bool,
) -> bool {
    let row_w = ui.available_width();
    let text_w = (row_w - TOGGLE_W - TOGGLE_GAP).max(1.0);

    let label_font = theme.ui_font(theme.text_label);
    let label_galley = ui
        .painter()
        .layout(label.to_owned(), label_font, theme.label, text_w);

    let label_h = label_galley.size().y;
    let hint_galley = hint.map(|hint| {
        let font = theme.ui_font(theme.text_caption);
        ui.painter()
            .layout(hint.to_owned(), font, theme.muted, text_w)
    });

    let hint_h = hint_galley.as_ref().map(|galley| galley.size().y);
    let text_h = toggle_text_height(label_h, hint_h);
    let inner_h = toggle_inner_height(text_h);
    let height = toggle_row_height(text_h);
    let (rect, _) = ui.allocate_exact_size(vec2(row_w, height), Sense::hover());
    let text_top = rect.top() + ROW_PAD_Y + (inner_h - text_h) * 0.5;

    ui.painter()
        .galley(pos2(rect.left(), text_top), label_galley, theme.label);

    if let Some(hint_galley) = hint_galley {
        let hint_y = text_top + label_h + HINT_GAP;
        ui.painter()
            .galley(pos2(rect.left(), hint_y), hint_galley, theme.muted);
    }

    let toggle_top = rect.top() + ROW_PAD_Y + (inner_h - TOGGLE_H) * 0.5;
    let toggle_rect = Rect::from_min_size(
        pos2(rect.right() - TOGGLE_W, toggle_top),
        vec2(TOGGLE_W, TOGGLE_H),
    );

    let mut knob = ui.new_child(UiBuilder::new().max_rect(toggle_rect));
    paint_switch(&mut knob, theme, *value);

    if !hit_on_top(ui, rect, label).clicked() {
        return false;
    }

    *value = !*value;
    true
}

fn toggle_text_height(label_h: f32, hint_h: Option<f32>) -> f32 {
    let Some(hint_h) = hint_h else {
        return label_h;
    };

    label_h + HINT_GAP + hint_h
}

fn toggle_inner_height(text_h: f32) -> f32 {
    text_h.max(TOGGLE_H)
}

fn toggle_row_height(text_h: f32) -> f32 {
    toggle_inner_height(text_h) + ROW_PAD_Y * 2.0
}

fn paint_switch(ui: &mut Ui, theme: &Theme, on: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(TOGGLE_W, TOGGLE_H), Sense::hover());
    let radius = CornerRadius::same((TOGGLE_H / 2.0).round() as u8);

    let fill = if on {
        theme.btn_primary_bg
    } else {
        theme.toggle_off
    };

    ui.painter().rect_filled(rect, radius, fill);

    let pad = 3.0;
    let knob_r = (TOGGLE_H - pad * 2.0) / 2.0;
    let knob_x = if on {
        rect.right() - pad - knob_r
    } else {
        rect.left() + pad + knob_r
    };

    let knob = if on {
        theme.btn_primary_fg
    } else {
        theme.title
    };

    let knob_pos = pos2(knob_x, rect.center().y);
    ui.painter().circle_filled(knob_pos, knob_r, knob);
}

pub fn text_row(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    hint: Option<&str>,
    placeholder: &str,
    value: &mut String,
) -> bool {
    field_label(ui, theme, label, hint);
    styled_edit(ui, theme, value, placeholder, false)
}

pub fn secret_row(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    hint: Option<&str>,
    secret: &mut SecretString,
) -> bool {
    field_label(ui, theme, label, hint);
    let mut value = secret.expose().to_owned();
    let changed = styled_edit(ui, theme, &mut value, "", true);
    if !changed {
        return false;
    }

    *secret = SecretString::from(value);
    true
}

pub fn select_row<T: Copy + PartialEq + std::fmt::Display>(
    ui: &mut Ui,
    theme: &Theme,
    id: &str,
    label: &str,
    hint: Option<&str>,
    value: &mut T,
    options: &[T],
) -> bool {
    field_label(ui, theme, label, hint);
    crate::widgets::combo::show(ui, theme, id, value, options)
}

pub struct Labeled<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub hint: Option<&'a str>,
}

pub fn select_row_with<T: Copy + PartialEq>(
    ui: &mut Ui,
    theme: &Theme,
    field: Labeled<'_>,
    value: &mut T,
    options: &[T],
    option_label: impl Fn(T) -> String,
) -> bool {
    field_label(ui, theme, field.label, field.hint);
    crate::widgets::combo::show_with(ui, theme, field.id, value, options, option_label)
}

pub fn multiselect_chip_row<T: Copy + PartialEq>(
    ui: &mut Ui,
    theme: &Theme,
    field: Labeled<'_>,
    selected: &mut Vec<T>,
    options: &[T],
    option_label: impl Fn(T) -> String,
) -> bool {
    field_label(ui, theme, field.label, field.hint);
    crate::widgets::chips::multi_row(ui, theme, selected, options, option_label)
}

pub fn probe_row<F, Fut>(
    ui: &mut Ui,
    theme: &Theme,
    icon: MaterialIcon,
    label: &str,
    bind: &mut Bind<String, JobError>,
    start: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<String, JobError>> + Send + 'static,
{
    ui.add_space(4.0);
    if action_button(ui, theme, icon, label, true) {
        bind.clear();
        bind.request(start());
    }
    show_probe(ui, bind, theme);
}

pub fn speed_test_row<F, Fut>(
    ui: &mut Ui,
    theme: &Theme,
    meter: &SpeedMeter,
    bind: &mut Bind<(), JobError>,
    start: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), JobError>> + Send + 'static,
{
    speed::paint(ui, theme, meter);
    ui.add_space(10.0);

    let busy = meter.is_busy();
    let clicked = action_button(ui, theme, ICON_SPEED, Msg::SpeedTest.t(), true);
    if clicked && !busy {
        meter.begin();
        bind.clear();
        bind.request(start());
    }

    if meter.needs_repaint() {
        ui.ctx().request_repaint();
    }
}

pub fn clear_cache_row(ui: &mut Ui, theme: &Theme) -> bool {
    ui.add_space(4.0);
    action_button(ui, theme, ICON_DELETE_SWEEP, Msg::ClearCache.t(), false)
}

fn field_label(ui: &mut Ui, theme: &Theme, label: &str, hint: Option<&str>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = HINT_GAP;
        ui.label(
            RichText::new(label)
                .size(theme.text_small)
                .color(theme.muted_bright),
        );

        let Some(hint) = hint else {
            return;
        };

        ui.label(
            RichText::new(hint)
                .size(theme.text_caption)
                .color(theme.muted),
        );
    });
}

fn styled_edit(
    ui: &mut Ui,
    theme: &Theme,
    value: &mut String,
    placeholder: &str,
    password: bool,
) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), INPUT_H), Sense::hover());
    ui.painter().rect(
        rect,
        theme.rounding(theme.radius_card),
        theme.input_bg,
        Stroke::new(1.0, theme.window_edge),
        egui::StrokeKind::Inside,
    );

    let inner = rect.shrink2(vec2(10.0, 0.0));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().interact_size.y = INPUT_H;

    let mut edit = TextEdit::singleline(value)
        .desired_width(f32::INFINITY)
        .vertical_align(Align::Center)
        .margin(Margin::ZERO)
        .hint_text(placeholder)
        .frame(Frame::NONE);

    if password {
        edit = edit.password(true);
    }

    row.add(edit).changed()
}

fn action_button(
    ui: &mut Ui,
    theme: &Theme,
    icon: MaterialIcon,
    label: &str,
    primary: bool,
) -> bool {
    let width = ui.available_width();
    let size = vec2(width, ACTION_H);
    let opts = action_opts(primary, size);
    let fg = action_fg(theme, primary);

    crate::widgets::button::add_named(
        ui,
        theme,
        (
            Atom::grow(),
            icon.rich_text().size(theme.text_icon_md).color(fg),
            RichText::new(label).size(theme.text_body).color(fg),
            Atom::grow(),
        ),
        opts,
        Some(label),
    )
    .clicked()
}

fn action_opts(primary: bool, size: Vec2) -> crate::widgets::button::Opts {
    if primary {
        return crate::widgets::button::Opts::primary(size);
    }

    crate::widgets::button::Opts::secondary(size)
}

fn action_fg(theme: &Theme, primary: bool) -> egui::Color32 {
    if primary {
        return theme.btn_primary_fg;
    }

    theme.title
}

fn hit_on_top(ui: &mut Ui, rect: egui::Rect, id: &str) -> egui::Response {
    ui.interact(rect, ui.id().with(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand)
}

fn show_probe(ui: &mut Ui, bind: &mut Bind<String, JobError>, theme: &Theme) {
    match bind.read() {
        None => {}
        Some(Ok(msg)) => {
            ui.label(RichText::new(msg).size(theme.text_small).color(theme.ok));
        }
        Some(Err(error)) => {
            let msg = error.to_string();
            ui.label(RichText::new(msg).size(theme.text_small).color(theme.err));
        }
    }
}
