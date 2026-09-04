//! Custom title bar: drag, window controls, back, settings, and search.

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
use crate::widgets::search::{self, SearchBar};

const RESIZE_GRIP: f32 = 6.0;
const SEARCH_INSET: f32 = 8.0;

pub fn header(
    ui: &mut Ui,
    screen: Screen,
    theme: &Theme,
    settings_open: bool,
    search: &mut SearchBar,
) -> Option<NavAction> {
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
            let show_back = settings_open || !matches!(screen, Screen::Home);
            if show_back {
                let back = chrome_btn(ui, theme, ICON_ARROW_BACK, Msg::NavBack.t(), false, false);
                if back {
                    action = Some(NavAction::GoBack);
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                window_buttons(ui, theme);
                if chrome_btn(
                    ui,
                    theme,
                    ICON_SETTINGS,
                    Msg::NavSettings.t(),
                    false,
                    settings_open,
                ) {
                    action = Some(NavAction::OpenSettings);
                }

                let remaining = ui.available_size();
                let (middle, _) = ui.allocate_exact_size(remaining, Sense::hover());
                if let Some(nav) = search_and_drag(ui, theme, search, bar, middle) {
                    action = Some(nav);
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
    if chrome_btn(ui, theme, ICON_CLOSE, Msg::WindowClose.t(), true, false) {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }

    let (max_icon, max_hint) = if maximized {
        (ICON_FULLSCREEN_EXIT, Msg::WindowRestore.t())
    } else {
        (ICON_FULLSCREEN, Msg::WindowMaximize.t())
    };

    if chrome_btn(ui, theme, max_icon, max_hint, false, false) {
        toggle_maximized(ui);
    }

    if chrome_btn(
        ui,
        theme,
        ICON_REMOVE,
        Msg::WindowMinimize.t(),
        false,
        false,
    ) {
        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
    }
}

fn toggle_maximized(ui: &Ui) {
    let maximized = ui.input(|i| i.viewport().maximized).unwrap_or(false);
    ui.ctx()
        .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
}

fn search_and_drag(
    ui: &mut Ui,
    theme: &Theme,
    search: &mut SearchBar,
    bar: Rect,
    middle: Rect,
) -> Option<NavAction> {
    let search_rect = centered_search_rect(bar, middle);

    let left_drag = Rect::from_min_max(middle.min, pos2(search_rect.left(), middle.bottom()));
    let right_drag = Rect::from_min_max(pos2(search_rect.right(), middle.top()), middle.max);

    title_drag(ui, left_drag, "left");
    title_drag(ui, right_drag, "right");

    search.show(ui, theme, search_rect)
}

fn centered_search_rect(bar: Rect, middle: Rect) -> Rect {
    let max_w = (middle.width() - SEARCH_INSET * 2.0).max(0.0);
    let search_w = search::SEARCH_W.min(max_w);
    let search_h = search::SEARCH_H.min(middle.height() - SEARCH_INSET).max(0.0);
    let desired = Rect::from_center_size(
        pos2(bar.center().x, middle.center().y),
        vec2(search_w, search_h),
    );

    let min_left = middle.left() + SEARCH_INSET;
    let max_right = middle.right() - SEARCH_INSET;
    let mut rect = desired;

    let overflows_left = rect.left() < min_left;
    if overflows_left {
        rect = rect.translate(vec2(min_left - rect.left(), 0.0));
    }

    let overflows_right = rect.right() > max_right;
    if overflows_right {
        rect = rect.translate(vec2(max_right - rect.right(), 0.0));
    }

    rect
}

fn title_drag(ui: &Ui, rect: Rect, id: &'static str) {
    if rect.width() < 1.0 {
        return;
    }

    let drag = ui.interact(
        rect,
        Id::new(("cinebox-title-drag", id)),
        Sense::click_and_drag(),
    );
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }

    if drag.double_clicked() {
        toggle_maximized(ui);
    }
}

fn chrome_btn(
    ui: &mut Ui,
    theme: &Theme,
    icon: MaterialIcon,
    hint: &str,
    is_close: bool,
    active: bool,
) -> bool {
    let size = Vec2::splat(theme.title_bar_h - 8.0);
    let idle = if active {
        theme.chrome_btn_hover
    } else {
        theme.chrome_btn_idle
    };

    let hover = if is_close {
        theme.chrome_close_hover
    } else {
        theme.chrome_btn_hover
    };

    let clicked = ui
        .scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.bg_fill = idle;
            widgets.inactive.weak_bg_fill = idle;
            widgets.hovered.bg_fill = hover;
            widgets.hovered.weak_bg_fill = hover;
            widgets.active.bg_fill = hover;
            widgets.active.weak_bg_fill = hover;

            ui.add(
                egui::Button::new(icon.rich_text().size(theme.text_icon).color(theme.title))
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(4)
                    .min_size(size),
            )
        })
        .inner;

    let enabled = clicked.enabled();
    clicked.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, hint));
    clicked
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(hint)
        .clicked()
}
