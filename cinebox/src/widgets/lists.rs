//! Icons and the add-to-list popover for user lists.

use cinebox_core::{LibraryList, LibraryMark, ListStatus};
use egui::{Align, Layout, Rect, RichText, Sense, Ui, vec2};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_BOOKMARK_ADD, ICON_CHECK, ICON_CHECK_CIRCLE, ICON_DELETE, ICON_DO_NOT_DISTURB_ON,
    ICON_FAVORITE, ICON_FAVORITE_BORDER, ICON_SCHEDULE, ICON_VISIBILITY,
};
use rust_i18n::t;

use crate::theme::Theme;
use crate::widgets::button;

const ROW_H: f32 = 36.0;
const MENU_W: f32 = 240.0;

#[must_use]
pub const fn status_icon(status: ListStatus) -> MaterialIcon {
    match status {
        ListStatus::Watching => ICON_VISIBILITY,
        ListStatus::Planned => ICON_SCHEDULE,
        ListStatus::Completed => ICON_CHECK_CIRCLE,
        ListStatus::Dropped => ICON_DO_NOT_DISTURB_ON,
    }
}

#[must_use]
pub const fn list_icon(list: LibraryList) -> MaterialIcon {
    match list {
        LibraryList::Status(status) => status_icon(status),
        LibraryList::Liked => ICON_FAVORITE,
    }
}

/// Icon for the media page's list button: the current status, or "add".
#[must_use]
pub const fn button_icon(mark: LibraryMark) -> MaterialIcon {
    match mark.status {
        Some(status) => status_icon(status),
        None if mark.liked => ICON_FAVORITE,
        None => ICON_BOOKMARK_ADD,
    }
}

/// What the user picked in the popover this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPick {
    Status(Option<ListStatus>),
    Liked(bool),
    Clear,
}

pub const fn menu_width() -> f32 {
    MENU_W
}

/// Popover body: one row per status (click the current one to clear it), liked toggle,
/// and "remove" when anything is set.
pub fn menu(ui: &mut Ui, theme: &Theme, mark: LibraryMark) -> Option<MenuPick> {
    let mut pick = None;
    ui.spacing_mut().item_spacing.y = 2.0;

    for status in ListStatus::ALL {
        let active = mark.status == Some(status);
        let label = crate::i18n::list_status_label(status);
        if row(ui, theme, status_icon(status), label.as_ref(), active, theme.title) {
            let next = if active { None } else { Some(status) };
            pick = Some(MenuPick::Status(next));
        }
    }

    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);

    let (heart, tint) = if mark.liked {
        (ICON_FAVORITE, theme.liked)
    } else {
        (ICON_FAVORITE_BORDER, theme.title)
    };
    if row(ui, theme, heart, t!("library.liked").as_ref(), mark.liked, tint) {
        pick = Some(MenuPick::Liked(!mark.liked));
    }

    if !mark.is_empty() && row(ui, theme, ICON_DELETE, t!("library.remove").as_ref(), false, theme.muted) {
        pick = Some(MenuPick::Clear);
    }

    pick
}

fn row(ui: &mut Ui, theme: &Theme, icon: MaterialIcon, label: &str, active: bool, tint: egui::Color32) -> bool {
    let size = vec2(ui.available_width(), ROW_H);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = button::pointing(response);
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, active, label));

    let fill = if response.hovered() {
        theme.widget_hover
    } else if active {
        theme.widget_active
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, theme.rounding(theme.radius_card), fill);

    let inner = Rect::from_min_max(rect.min + vec2(10.0, 0.0), rect.max - vec2(10.0, 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.label(icon.rich_text().size(theme.text_icon_md).color(tint));
            ui.label(RichText::new(label).size(theme.text_body).color(theme.title));
            if active {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(ICON_CHECK.rich_text().size(theme.text_icon).color(theme.title));
                });
            }
        },
    );

    response.clicked()
}
