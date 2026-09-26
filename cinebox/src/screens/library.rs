//! "My lists": the user's status lists and liked titles, filterable by section.

use cinebox_core::{LibraryList, ListStatus, Section};
use egui::{RichText, Ui, vec2};
use rust_i18n::t;

use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::{lists, poster, scroll};

pub struct LibraryScreen {
    list: LibraryList,
    section: Option<Section>,
    reset_scroll: bool,
}

impl Default for LibraryScreen {
    fn default() -> Self {
        Self {
            list: LibraryList::Status(ListStatus::Watching),
            section: None,
            reset_scroll: false,
        }
    }
}

impl LibraryScreen {
    pub fn seed(&mut self, list: LibraryList) {
        self.list = list;
        self.reset_scroll = true;
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme) -> Option<NavAction> {
        let to_top = std::mem::take(&mut self.reset_scroll);
        let mut action = None;

        let body = |ui: &mut Ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new(t!("library.title").as_ref())
                    .font(theme.title_font(theme.text_heading))
                    .color(theme.title),
            );
            ui.add_space(10.0);

            if let Some(list) = list_tabs(ui, svc, theme, self.list, self.section) {
                self.list = list;
            }
            ui.add_space(6.0);
            if let Some(section) = section_tabs(ui, theme, self.section) {
                self.section = section;
            }
            ui.add_space(12.0);

            let entries = svc.library.list(self.list, self.section);
            if entries.is_empty() {
                empty_state(ui, theme);
                return;
            }

            let scale = poster::card_scale(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                for entry in entries {
                    let item = &entry.item;
                    let marks = svc.tile_marks(item.kind, item.id);
                    let size = svc.settings.tmdb.poster_size;
                    let opened = poster::catalog_tile(ui, item, &svc.images, size, theme, marks, scale);
                    if action.is_none() {
                        action = opened;
                    }
                }
            });
        };

        if to_top {
            scroll::vertical_to_top(ui, "library-page", body);
        } else {
            scroll::vertical(ui, "library-page", body);
        }

        action
    }
}

fn list_tabs(
    ui: &mut Ui,
    svc: &Services,
    theme: &Theme,
    current: LibraryList,
    section: Option<Section>,
) -> Option<LibraryList> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for list in LibraryList::ALL {
            let icon = lists::list_icon(list);
            let count = svc.library.count(list, section);
            let label = format!("{} {count}", crate::i18n::library_list_label(list));
            let pad_y = button::icon_label_pad_y(ui, theme, icon, button::CHIP_H);
            let opts = Opts::chip(list == current).pad_y(pad_y);

            if button::icon_label(ui, theme, icon, &label, opts) {
                picked = Some(list);
            }
        }
    });

    picked
}

/// `Some(None)` means "all sections".
fn section_tabs(ui: &mut Ui, theme: &Theme, current: Option<Section>) -> Option<Option<Section>> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if button::label(ui, theme, t!("library.all").as_ref(), Opts::chip(current.is_none())) {
            picked = Some(None);
        }

        for section in Section::ALL {
            let label = crate::i18n::section_title(section);
            if button::label(ui, theme, label.as_ref(), Opts::chip(current == Some(section))) {
                picked = Some(Some(section));
            }
        }
    });

    picked
}

fn empty_state(ui: &mut Ui, theme: &Theme) {
    ui.add_space(24.0);
    ui.label(
        RichText::new(t!("catalog.empty").as_ref())
            .font(theme.title_font(theme.text_display))
            .color(theme.muted),
    );
    ui.add_space(6.0);
    ui.label(
        RichText::new(t!("library.empty_hint").as_ref())
            .size(theme.text_body)
            .color(theme.muted),
    );
}
