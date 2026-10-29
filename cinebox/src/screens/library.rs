//! "My lists": the user's status lists and liked titles, filterable by section.

use cinebox_core::{LibraryList, ListStatus, Section};
use egui::{RichText, Ui};
use rust_i18n::t;

use super::paged;
use crate::app::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::{lists, scroll};

pub(crate) struct LibraryScreen {
    list: LibraryList,
    section: Option<Section>,
    is_scroll_reset: bool,
}

/// A chip picked in [`section_tabs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionPick {
    All,
    One(Section),
}

impl Default for LibraryScreen {
    fn default() -> Self {
        Self { list: LibraryList::Status(ListStatus::Watching), section: None, is_scroll_reset: false }
    }
}

impl LibraryScreen {
    pub(crate) fn seed(&mut self, list: LibraryList) {
        self.list = list;
        self.is_scroll_reset = true;
    }

    pub(crate) fn ui(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme) -> Option<NavAction> {
        let is_scroll_reset = std::mem::take(&mut self.is_scroll_reset);
        let mut action = None;

        scroll::page(ui, "library-page", is_scroll_reset, |ui| {
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

            match section_tabs(ui, theme, self.section) {
                Some(SectionPick::All) => self.section = None,
                Some(SectionPick::One(section)) => self.section = Some(section),
                None => {}
            }

            ui.add_space(12.0);

            let entries = svc.library.list(self.list, self.section);
            if entries.is_empty() {
                empty_state(ui, theme);
                return;
            }

            action = paged::tile_grid(ui, svc, theme, entries.iter().map(|entry| &entry.item));
        });

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

fn section_tabs(ui: &mut Ui, theme: &Theme, current: Option<Section>) -> Option<SectionPick> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;

        if button::label(ui, theme, t!("library.all").as_ref(), Opts::chip(current.is_none())) {
            picked = Some(SectionPick::All);
        }

        for section in Section::ALL {
            let label = crate::i18n::section_title(section);
            if button::label(ui, theme, label.as_ref(), Opts::chip(current == Some(section))) {
                picked = Some(SectionPick::One(section));
            }
        }
    });

    picked
}

fn empty_state(ui: &mut Ui, theme: &Theme) {
    ui.add_space(24.0);
    ui.label(RichText::new(t!("catalog.empty").as_ref()).font(theme.title_font(theme.text_display)).color(theme.muted));
    ui.add_space(6.0);
    ui.label(RichText::new(t!("library.empty_hint").as_ref()).size(theme.text_body).color(theme.muted));
}
