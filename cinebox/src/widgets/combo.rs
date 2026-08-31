//! ComboBox chrome shared by settings and torrent filters.

use egui::style::StyleModifier;
use egui::{Color32, ComboBox, CursorIcon, Margin, RichText, Stroke, TextStyle, Ui, vec2};

use super::button::{PAD_X, PAD_Y};

use crate::theme::Theme;

pub const HEIGHT: f32 = super::button::CONTROL_H;

pub fn apply_visuals(ui: &mut Ui, theme: &Theme) {
    ui.spacing_mut().interact_size.y = HEIGHT;
    ui.spacing_mut().button_padding = vec2(PAD_X, pad_y(ui));
    ui.style_mut().interaction.selectable_labels = false;

    let radius = theme.rounding(theme.radius_card);
    let stroke = Stroke::new(1.0, theme.window_edge);
    let widgets = &mut ui.visuals_mut().widgets;

    widgets.inactive.weak_bg_fill = theme.input_bg;
    widgets.inactive.bg_fill = theme.input_bg;
    widgets.inactive.bg_stroke = stroke;
    widgets.inactive.corner_radius = radius;
    widgets.inactive.expansion = 0.0;
    widgets.hovered.weak_bg_fill = theme.widget_hover;
    widgets.hovered.bg_fill = theme.widget_hover;
    widgets.hovered.bg_stroke = stroke;
    widgets.hovered.corner_radius = radius;
    widgets.hovered.expansion = 0.0;
    widgets.active.weak_bg_fill = theme.widget_active;
    widgets.active.bg_fill = theme.widget_active;
    widgets.active.corner_radius = radius;
    widgets.active.expansion = 0.0;
    widgets.open.weak_bg_fill = theme.input_bg;
    widgets.open.bg_stroke = stroke;
    widgets.open.corner_radius = radius;
}

/// Vertical padding that keeps the closed combo exactly [`HEIGHT`] tall:
/// `max(text row, arrow icon) + 2 * PAD_Y` would overshoot it.
fn pad_y(ui: &Ui) -> f32 {
    let text_font = TextStyle::Button.resolve(ui.style());
    let text_h = ui.ctx().fonts_mut(|f| f.row_height(&text_font));
    let content_h = text_h.max(ui.spacing().icon_width);

    ((HEIGHT - content_h) / 2.0).clamp(0.0, PAD_Y)
}

pub fn popup_style(theme: &Theme) -> StyleModifier {
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
        style.interaction.selectable_labels = false;
        style.visuals.interact_cursor = Some(CursorIcon::PointingHand);
        style.spacing.item_spacing.y = 4.0;
        style.spacing.button_padding = vec2(PAD_X, PAD_Y);
        style.spacing.menu_margin = Margin::symmetric(8, 8);
    })
}

pub fn show<T: Copy + PartialEq + std::fmt::Display>(
    ui: &mut Ui,
    theme: &Theme,
    id: &str,
    value: &mut T,
    options: &[T],
) -> bool {
    show_with(ui, theme, id, value, options, |opt| opt.to_string())
}

pub fn show_with<T: Copy + PartialEq>(
    ui: &mut Ui,
    theme: &Theme,
    id: &str,
    value: &mut T,
    options: &[T],
    label: impl Fn(T) -> String,
) -> bool {
    let mut changed = false;
    let width = ui.available_width();
    let selected = RichText::new(label(*value)).color(theme.label);

    ui.scope(|ui| {
        apply_visuals(ui, theme);
        ComboBox::from_id_salt(id)
            .width(width)
            .selected_text(selected)
            .popup_style(popup_style(theme))
            .show_ui(ui, |ui| {
                for opt in options {
                    let clicked = ui.selectable_value(value, *opt, label(*opt));
                    changed |= clicked.changed();
                }
            })
            .response
            .on_hover_cursor(CursorIcon::PointingHand);
    });

    changed
}
