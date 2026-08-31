//! Shared buttons and clickable-row helpers.
//!
//! Never call [`egui::Button::fill`]: it freezes the background and kills hover.

use egui::{
    Atom, Color32, CursorIcon, Id, IntoAtoms, Response, RichText, Sense, Stroke, Ui, Vec2,
    WidgetInfo, WidgetType, vec2,
};
use egui_material_icons::MaterialIcon;

use crate::theme::Theme;

pub const PAD_X: f32 = 14.0;
pub const PAD_Y: f32 = 8.0;
pub const CHIP_MIN_W: f32 = 88.0;

pub const CONTROL_H: f32 = 32.0;
pub const CHIP_H: f32 = CONTROL_H;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub tone: Tone,
    pub min_size: Vec2,
    pub selected: bool,
    pub gap: f32,
}

impl Opts {
    #[must_use]
    pub fn primary(min_size: Vec2) -> Self {
        Self {
            tone: Tone::Primary,
            min_size,
            selected: false,
            gap: 8.0,
        }
    }

    #[must_use]
    pub fn secondary(min_size: Vec2) -> Self {
        Self {
            tone: Tone::Secondary,
            min_size,
            selected: false,
            gap: 6.0,
        }
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn chip(active: bool) -> Self {
        if active {
            return Self::primary(vec2(CHIP_MIN_W, CHIP_H));
        }

        Self::secondary(vec2(CHIP_MIN_W, CHIP_H))
    }
}

pub fn pointing(response: Response) -> Response {
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Idle vs hover fill for clickable cards (previous frame’s hover).
#[must_use]
pub fn fill_for_hover(ui: &Ui, id: Id, idle: Color32, hover: Color32) -> Color32 {
    let hovered = ui
        .ctx()
        .read_response(id)
        .is_some_and(|response| response.hovered() || response.contains_pointer());

    if hovered {
        return hover;
    }

    idle
}

pub fn click_rect(ui: &mut Ui, id: Id, rect: egui::Rect) -> Response {
    pointing(ui.interact(rect, id, Sense::click()))
}

pub fn add<'a>(ui: &mut Ui, theme: &Theme, atoms: impl IntoAtoms<'a>, opts: Opts) -> Response {
    add_named(ui, theme, atoms, opts, None)
}

pub fn add_named<'a>(
    ui: &mut Ui,
    theme: &Theme,
    atoms: impl IntoAtoms<'a>,
    opts: Opts,
    name: Option<&str>,
) -> Response {
    let (idle, hover, active, stroke) = palette(theme, &opts);

    let response = ui
        .scope(|ui| {
            paint_visuals(ui, theme, idle, hover, active, stroke, opts.min_size.y);
            pointing(
                ui.add(
                    egui::Button::new(atoms)
                        .stroke(stroke)
                        .gap(opts.gap)
                        .corner_radius(theme.rounding(theme.radius_card))
                        .min_size(opts.min_size),
                ),
            )
        })
        .inner;

    let Some(name) = name else {
        return response;
    };

    announce(response, name)
}

fn announce(response: Response, label: &str) -> Response {
    let enabled = response.enabled();
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
    response
}

pub fn icon_label(ui: &mut Ui, theme: &Theme, icon: MaterialIcon, label: &str, opts: Opts) -> bool {
    let fg = foreground(theme, opts.tone);
    let atoms = (
        Atom::grow(),
        icon.rich_text().size(theme.text_icon).color(fg),
        RichText::new(label).size(theme.text_body).color(fg),
        Atom::grow(),
    );

    add_named(ui, theme, atoms, opts, Some(label)).clicked()
}

pub fn label(ui: &mut Ui, theme: &Theme, text: &str, opts: Opts) -> bool {
    let fg = foreground(theme, opts.tone);
    add_named(
        ui,
        theme,
        RichText::new(text).size(theme.text_body).color(fg),
        opts,
        Some(text),
    )
    .clicked()
}

fn foreground(theme: &Theme, tone: Tone) -> Color32 {
    match tone {
        Tone::Primary => theme.btn_primary_fg,
        Tone::Secondary => theme.title,
    }
}

fn palette(theme: &Theme, opts: &Opts) -> (Color32, Color32, Color32, Stroke) {
    match opts.tone {
        Tone::Primary => primary_palette(theme, opts.selected),
        Tone::Secondary => secondary_palette(theme, opts.selected),
    }
}

fn primary_palette(theme: &Theme, selected: bool) -> (Color32, Color32, Color32, Stroke) {
    if selected {
        return (
            theme.btn_primary_hover,
            theme.btn_primary_hover,
            theme.btn_primary_hover,
            Stroke::NONE,
        );
    }

    (
        theme.btn_primary_bg,
        theme.btn_primary_hover,
        theme.btn_primary_hover,
        Stroke::NONE,
    )
}

fn secondary_palette(theme: &Theme, selected: bool) -> (Color32, Color32, Color32, Stroke) {
    let stroke = Stroke::new(1.0, theme.window_edge);
    if selected {
        return (
            theme.widget_hover,
            theme.widget_active,
            theme.widget_active,
            stroke,
        );
    }

    (
        theme.input_bg,
        theme.widget_hover,
        theme.widget_active,
        stroke,
    )
}

fn paint_visuals(
    ui: &mut Ui,
    theme: &Theme,
    idle: Color32,
    hover: Color32,
    active: Color32,
    stroke: Stroke,
    min_h: f32,
) {
    ui.spacing_mut().button_padding = vec2(PAD_X, PAD_Y);
    if min_h > 0.0 {
        ui.spacing_mut().interact_size.y = min_h;
    }

    let radius = theme.rounding(theme.radius_card);
    let widgets = &mut ui.visuals_mut().widgets;

    widgets.inactive.weak_bg_fill = idle;
    widgets.inactive.bg_fill = idle;
    widgets.inactive.bg_stroke = stroke;
    widgets.inactive.corner_radius = radius;
    widgets.inactive.expansion = 0.0;
    widgets.hovered.weak_bg_fill = hover;
    widgets.hovered.bg_fill = hover;
    widgets.hovered.bg_stroke = stroke;
    widgets.hovered.corner_radius = radius;
    widgets.hovered.expansion = 0.0;
    widgets.active.weak_bg_fill = active;
    widgets.active.bg_fill = active;
    widgets.active.bg_stroke = stroke;
    widgets.active.corner_radius = radius;
    widgets.active.expansion = 0.0;
    widgets.open.weak_bg_fill = idle;
    widgets.open.bg_fill = idle;
    widgets.open.bg_stroke = stroke;
    widgets.open.corner_radius = radius;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_for_hover_idles_without_response() {
        let idle = Color32::from_rgb(1, 2, 3);
        let hover = Color32::from_rgb(4, 5, 6);

        assert_eq!(idle_or_hover(false, idle, hover), idle);
        assert_eq!(idle_or_hover(true, idle, hover), hover);
    }

    fn idle_or_hover(hovered: bool, idle: Color32, hover: Color32) -> Color32 {
        if hovered {
            return hover;
        }

        idle
    }
}
