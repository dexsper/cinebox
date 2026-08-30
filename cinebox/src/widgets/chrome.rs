//! Custom title bar: drag, window controls, back, and settings.

use cinebox_core::i18n::Msg;
use egui::{
    CursorIcon, Id, Rect, Sense, Ui, Vec2, ViewportCommand, pos2, vec2, viewport::ResizeDirection,
};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_ARROW_BACK, ICON_CLOSE, ICON_FULLSCREEN, ICON_FULLSCREEN_EXIT, ICON_REMOVE, ICON_SETTINGS,
};

use crate::nav::{NavAction, Screen};
use crate::theme::Theme;

const RESIZE_GRIP: f32 = 6.0;

pub fn header(ui: &mut Ui, screen: Screen, theme: &Theme) -> Option<NavAction> {
    let mut action = None;
    let height = theme.title_bar_h;
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    
    ui.painter().rect_filled(bar, 0.0, theme.chrome_bg);
    ui.painter().hline(
        bar.x_range(),
        bar.bottom() - 1.0,
        egui::Stroke::new(1.0, theme.window_edge),
    );

    ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            if !matches!(screen, Screen::Home)
                && chrome_btn(ui, theme, ICON_ARROW_BACK, Msg::NavBack.en(), false)
            {
                action = Some(NavAction::GoBack);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                window_buttons(ui, theme);
                if chrome_btn(ui, theme, ICON_SETTINGS, Msg::NavSettings.en(), false) {
                    action = Some(NavAction::OpenSettings);
                }

                let remaining = ui.available_size();
                let (_, drag) = ui.allocate_exact_size(remaining, Sense::click_and_drag());
                if drag.drag_started() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                }
                
                if drag.double_clicked() {
                    toggle_maximized(ui);
                }
            });
        });
    });

    action
}

/// 1px edge so an undecorated window still has a clear outline.
pub fn window_outline(ui: &Ui, theme: &Theme) {
    ui.painter().rect_stroke(
        ui.max_rect(),
        0.0,
        egui::Stroke::new(1.0, theme.window_edge),
        egui::StrokeKind::Inside,
    );
}

/// Side and bottom resize grips. Skips the title bar so chrome buttons stay clickable.
pub fn resize_edges(ui: &Ui, theme: &Theme) {
    if ui.input(|i| i.viewport().maximized).unwrap_or(false) {
        return;
    }

    let rect = ui.max_rect();
    let g = RESIZE_GRIP;
    let top = rect.top() + theme.title_bar_h;

    hit_resize(
        ui,
        Rect::from_min_max(
            pos2(rect.left(), top),
            pos2(rect.left() + g, rect.bottom() - g),
        ),
        ResizeDirection::West,
        CursorIcon::ResizeWest,
        "w",
    );
    hit_resize(
        ui,
        Rect::from_min_max(
            pos2(rect.right() - g, top),
            pos2(rect.right(), rect.bottom() - g),
        ),
        ResizeDirection::East,
        CursorIcon::ResizeEast,
        "e",
    );
    hit_resize(
        ui,
        Rect::from_min_max(
            pos2(rect.left() + g, rect.bottom() - g),
            pos2(rect.right() - g, rect.bottom()),
        ),
        ResizeDirection::South,
        CursorIcon::ResizeSouth,
        "s",
    );
    hit_resize(
        ui,
        Rect::from_min_max(
            pos2(rect.left(), rect.bottom() - g),
            pos2(rect.left() + g, rect.bottom()),
        ),
        ResizeDirection::SouthWest,
        CursorIcon::ResizeSouthWest,
        "sw",
    );
    hit_resize(
        ui,
        Rect::from_min_max(
            pos2(rect.right() - g, rect.bottom() - g),
            pos2(rect.right(), rect.bottom()),
        ),
        ResizeDirection::SouthEast,
        CursorIcon::ResizeSouthEast,
        "se",
    );
}

fn hit_resize(ui: &Ui, rect: Rect, dir: ResizeDirection, cursor: CursorIcon, id: &'static str) {
    let response = ui.interact(rect, Id::new(("cinebox-resize", id)), Sense::drag());
    if response.hovered() {
        ui.ctx().set_cursor_icon(cursor);
    }
    if response.drag_started() {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::BeginResize(dir));
    }
}

fn window_buttons(ui: &mut Ui, theme: &Theme) {
    let maximized = ui.input(|i| i.viewport().maximized).unwrap_or(false);
    if chrome_btn(ui, theme, ICON_CLOSE, Msg::WindowClose.en(), true) {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }

    let (max_icon, max_hint) = if maximized {
        (ICON_FULLSCREEN_EXIT, Msg::WindowRestore.en())
    } else {
        (ICON_FULLSCREEN, Msg::WindowMaximize.en())
    };

    if chrome_btn(ui, theme, max_icon, max_hint, false) {
        toggle_maximized(ui);
    }

    if chrome_btn(ui, theme, ICON_REMOVE, Msg::WindowMinimize.en(), false) {
        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
    }
}

fn toggle_maximized(ui: &Ui) {
    let maximized = ui.input(|i| i.viewport().maximized).unwrap_or(false);
    ui.ctx()
        .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
}

fn chrome_btn(ui: &mut Ui, theme: &Theme, icon: MaterialIcon, hint: &str, is_close: bool) -> bool {
    let size = Vec2::splat(theme.title_bar_h - 8.0);
    let clicked = ui
        .scope(|ui| {
            let hover = if is_close {
                theme.chrome_close_hover
            } else {
                theme.chrome_btn_hover
            };
            ui.visuals_mut().widgets.hovered.bg_fill = hover;
            ui.visuals_mut().widgets.active.bg_fill = hover;
            ui.add(
                egui::Button::new(icon.rich_text().size(16.0).color(theme.title))
                    .fill(theme.chrome_btn_idle)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(4)
                    .min_size(size),
            )
        })
        .inner;
    let enabled = clicked.enabled();
    clicked.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, hint));
    clicked.on_hover_text(hint).clicked()
}
