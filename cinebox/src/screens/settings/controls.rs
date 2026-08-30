//! Shared styled controls for the settings drawer.

use cinebox_core::SecretString;
use egui::{
    Align, Atom, Color32, ComboBox, CornerRadius, CursorIcon, Frame, Layout, Margin, RichText,
    Sense, Stroke, TextEdit, Ui, UiBuilder, Vec2, pos2, style::StyleModifier, vec2,
};
use egui_async::Bind;
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_ARROW_BACK, ICON_CHEVRON_RIGHT, ICON_DELETE_SWEEP, ICON_SPEED,
};

use super::catalog::Category;
use super::speed::{self, SpeedMeter};
use crate::theme::Theme;

const CATEGORY_H: f32 = 72.0;
const ICON_WELL: f32 = 40.0;
const TOGGLE_W: f32 = 44.0;
const TOGGLE_H: f32 = 26.0;
const INPUT_H: f32 = 32.0;
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
    row.add_space(12.0);
    row.vertical(|ui| {
        ui.label(
            RichText::new(cat.title)
                .font(theme.title_font(theme.text_section))
                .color(theme.title),
        );
        ui.label(RichText::new(cat.subtitle).size(theme.text_small).color(theme.muted));
    });
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(ICON_CHEVRON_RIGHT.rich_text().size(theme.text_icon_lg).color(theme.muted));
    });

    // Last so it sits above labels and eats the click instead of text selection.
    hit_on_top(ui, rect, cat.title).clicked()
}

fn icon_well(ui: &mut Ui, theme: &Theme, cat: &Category) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(ICON_WELL), Sense::hover());
    ui.painter()
        .rect_filled(rect, theme.rounding(theme.radius_card), theme.input_bg);
    ui.new_child(UiBuilder::new().max_rect(rect))
        .centered_and_justified(|ui| {
            ui.label(cat.icon.rich_text().size(theme.text_icon_lg).color(theme.title));
        });
}

pub fn nav_header(ui: &mut Ui, theme: &Theme, title: &str) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );

    let back = row
        .add(
            egui::Button::new(ICON_ARROW_BACK.rich_text().size(theme.text_icon_md).color(theme.title))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE)
                .corner_radius(6)
                .min_size(vec2(32.0, 32.0)),
        )
        .on_hover_text("Back")
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
    let mut height = 48.0;
    if hint.is_some() {
        height = 64.0;
    }

    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if ui.rect_contains_pointer(rect) {
        ui.painter()
            .rect_filled(rect, theme.rounding(theme.radius_card), theme.widget_hover);
    }

    let mut row = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 8.0))));
    row.style_mut().interaction.selectable_labels = false;
    row.horizontal_centered(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(label).size(theme.text_label).color(theme.label));
            let Some(hint) = hint else {
                return;
            };
            ui.label(RichText::new(hint).size(theme.text_caption).color(theme.muted));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            paint_switch(ui, theme, *value);
        });
    });

    if !hit_on_top(ui, rect, label).clicked() {
        return false;
    }

    *value = !*value;
    true
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
    ui.painter()
        .circle_filled(pos2(knob_x, rect.center().y), knob_r, knob);
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
    let mut changed = false;
    let width = ui.available_width();

    ui.scope(|ui| {
        apply_combo_visuals(ui, theme);
        ComboBox::from_id_salt(id)
            .width(width)
            .selected_text(RichText::new(value.to_string()).color(theme.label))
            .popup_style(combo_popup_style(theme))
            .show_ui(ui, |ui| {
                for opt in options {
                    changed |= ui.selectable_value(value, *opt, opt.to_string()).changed();
                }
            });
    });

    changed
}

pub fn data_language_row(ui: &mut Ui, theme: &Theme, value: &mut Option<String>) -> bool {
    field_label(
        ui,
        theme,
        "Data language",
        Some("Empty uses the OS language later."),
    );
    let mut lang = value.clone().unwrap_or_default();
    let changed = styled_edit(ui, theme, &mut lang, "en-US", false);
    if !changed {
        return false;
    }

    let trimmed = lang.trim();
    if trimmed.is_empty() {
        *value = None;
        return true;
    }

    *value = Some(trimmed.to_owned());
    true
}

pub fn probe_row<F, Fut>(
    ui: &mut Ui,
    theme: &Theme,
    icon: MaterialIcon,
    label: &str,
    bind: &mut Bind<String, String>,
    start: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
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
    bind: &mut Bind<(), String>,
    start: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
{
    speed::paint(ui, theme, meter);
    ui.add_space(10.0);

    let busy = meter.is_busy();
    let clicked = action_button(ui, theme, ICON_SPEED, "Speed Test", true);
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
    action_button(
        ui,
        theme,
        ICON_DELETE_SWEEP,
        cinebox_core::i18n::Msg::ClearCache.en(),
        false,
    )
}

fn apply_combo_visuals(ui: &mut Ui, theme: &Theme) {
    ui.spacing_mut().interact_size.y = INPUT_H;
    ui.spacing_mut().button_padding = vec2(10.0, 6.0);

    let radius = theme.rounding(theme.radius_card);
    let stroke = Stroke::new(1.0, theme.window_edge);
    let widgets = &mut ui.visuals_mut().widgets;

    widgets.inactive.weak_bg_fill = theme.input_bg;
    widgets.inactive.bg_fill = theme.input_bg;
    widgets.inactive.bg_stroke = stroke;
    widgets.inactive.corner_radius = radius;
    widgets.hovered.weak_bg_fill = theme.widget_hover;
    widgets.hovered.bg_stroke = stroke;
    widgets.hovered.corner_radius = radius;
    widgets.active.weak_bg_fill = theme.widget_active;
    widgets.active.corner_radius = radius;
    widgets.open.weak_bg_fill = theme.input_bg;
    widgets.open.bg_stroke = stroke;
    widgets.open.corner_radius = radius;
}

fn combo_popup_style(theme: &Theme) -> StyleModifier {
    let fill = theme.panel_elevated;
    let hover = theme.widget_hover;

    StyleModifier::new(move |style| {
        style.visuals.window_fill = fill;
        style.visuals.panel_fill = fill;
        style.visuals.extreme_bg_color = fill;
        style.visuals.faint_bg_color = fill;
        style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.hovered.weak_bg_fill = hover;
        style.visuals.widgets.hovered.bg_fill = hover;
        style.spacing.item_spacing.y = 4.0;
        style.spacing.button_padding = vec2(10.0, 8.0);
        style.spacing.menu_margin = Margin::symmetric(8, 8);
    })
}

fn field_label(ui: &mut Ui, theme: &Theme, label: &str, hint: Option<&str>) {
    ui.add_space(4.0);
    ui.label(RichText::new(label).size(theme.text_small).color(theme.muted_bright));
    let Some(hint) = hint else {
        return;
    };
    ui.label(RichText::new(hint).size(theme.text_caption).color(theme.muted));
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
    ui.scope(|ui| {
        let (fill, fg, hover) = if primary {
            (
                theme.btn_primary_bg,
                theme.btn_primary_fg,
                theme.btn_primary_hover,
            )
        } else {
            (theme.input_bg, theme.title, theme.widget_hover)
        };

        ui.visuals_mut().widgets.inactive.bg_fill = fill;
        ui.visuals_mut().widgets.hovered.bg_fill = hover;
        ui.visuals_mut().widgets.active.bg_fill = hover;
        ui.add(
            egui::Button::new((
                Atom::grow(),
                icon.rich_text().size(theme.text_icon_md).color(fg),
                RichText::new(label).size(theme.text_body).color(fg),
                Atom::grow(),
            ))
            .fill(fill)
            .stroke(Stroke::NONE)
            .gap(8.0)
            .corner_radius(theme.rounding(theme.radius_card))
            .min_size(vec2(width, ACTION_H)),
        )
        .clicked()
    })
    .inner
}

fn hit_on_top(ui: &mut Ui, rect: egui::Rect, id: &str) -> egui::Response {
    ui.interact(rect, ui.id().with(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand)
}

fn show_probe(ui: &mut Ui, bind: &mut Bind<String, String>, theme: &Theme) {
    match bind.read() {
        None => {}
        Some(Ok(msg)) => {
            ui.label(RichText::new(msg).size(theme.text_small).color(theme.ok));
        }
        Some(Err(msg)) => {
            ui.label(RichText::new(msg).size(theme.text_small).color(theme.err));
        }
    }
}
