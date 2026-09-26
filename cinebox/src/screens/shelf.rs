//! Horizontal poster shelf with a clickable heading, shared by Home and section hubs.

use cinebox_core::{CatalogItem, HomeRowId, LibraryList, ListStatus};
use cinebox_tmdb::ShelfId;
use egui::{FontId, RichText, Sense, Ui, WidgetInfo, WidgetType, pos2, vec2};
use egui_material_icons::icons::ICON_CHEVRON_RIGHT;
use rust_i18n::t;

use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::button::pointing;
use crate::widgets::{poster, scroll};

pub fn shelf(
    ui: &mut Ui,
    id: ShelfId,
    items: &[CatalogItem],
    error: Option<&str>,
    svc: &Services,
    theme: &Theme,
) -> Option<NavAction> {
    ui.add_space(12.0);

    let mut action = None;
    if shelf_heading(ui, crate::i18n::shelf_title(id).as_ref(), theme) {
        action = Some(open_shelf(id, items));
    }

    if let Some(error) = error {
        ui.label(RichText::new(error).size(theme.text_small).color(theme.err));
    }

    if items.is_empty() {
        if error.is_none() {
            ui.label(
                RichText::new(t!("catalog.empty").as_ref())
                    .size(theme.text_small)
                    .color(theme.muted),
            );
        }
        return action;
    }

    let scale = poster::card_scale(ui.available_width());
    scroll::horizontal(ui, ("shelf", id.as_key()), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for item in items {
                let marks = svc.tile_marks(item.kind, item.id);
                let size = svc.settings.tmdb.poster_size;
                let opened = poster::catalog_tile(ui, item, &svc.images, size, theme, marks, scale);
                if action.is_none() {
                    action = opened;
                }
            }
        });
    });
    action
}

/// List shelves open the library tab rather than a copy of it.
fn open_shelf(id: ShelfId, items: &[CatalogItem]) -> NavAction {
    let status = match id {
        ShelfId::Home(HomeRowId::Watching) => Some(ListStatus::Watching),
        ShelfId::Home(HomeRowId::Planned) => Some(ListStatus::Planned),
        _ => None,
    };

    if let Some(status) = status {
        return NavAction::OpenLibrary {
            list: LibraryList::Status(status),
        };
    }

    NavAction::OpenCategory {
        id,
        items: items.to_vec(),
    }
}

pub(crate) fn shelf_heading(ui: &mut Ui, title: &str, theme: &Theme) -> bool {
    let icon = ICON_CHEVRON_RIGHT;
    let title_font = theme.title_font(theme.text_section);
    let icon_font = FontId::new(theme.text_icon_md, icon.font_family());
    let title_galley = ui
        .painter()
        .layout_no_wrap(title.to_owned(), title_font, theme.title);

    let icon_galley =
        ui.painter()
            .layout_no_wrap(icon.codepoint.to_owned(), icon_font, theme.muted);

    let gap = 4.0;
    let width = title_galley.size().x + gap + icon_galley.size().x;
    let height = title_galley.size().y.max(icon_galley.size().y);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let response = pointing(response);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, title));

    let title_pos = pos2(rect.left(), rect.center().y - title_galley.size().y * 0.5);
    ui.painter().galley(title_pos, title_galley, theme.title);

    let icon_pos = pos2(
        rect.right() - icon_galley.size().x,
        rect.center().y - icon_galley.size().y * 0.5,
    );

    ui.painter().galley(icon_pos, icon_galley, theme.muted);
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watching_shelf_opens_library_tab() {
        let action = open_shelf(ShelfId::Home(HomeRowId::Watching), &[]);

        assert_eq!(
            action,
            NavAction::OpenLibrary {
                list: LibraryList::Status(ListStatus::Watching)
            }
        );
    }

    #[test]
    fn remote_shelf_opens_category_grid() {
        let id = ShelfId::Home(HomeRowId::NowPlaying);

        assert_eq!(
            open_shelf(id, &[]),
            NavAction::OpenCategory {
                id,
                items: Vec::new()
            }
        );
    }
}
