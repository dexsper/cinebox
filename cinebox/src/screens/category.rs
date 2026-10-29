//! Full-shelf grid: items already on the shelf, then extra TMDB pages on scroll.

use cinebox_core::{CatalogItem, UiLanguage};
use cinebox_tmdb::ShelfId;
use egui::{RichText, Ui};

use super::gate;
use crate::app::nav::NavAction;
use crate::screens::paged::{GridView, PagedGrid, Source};
use crate::services::Services;
use crate::services::jobs;
use crate::theme::Theme;

#[derive(Default)]
pub(crate) struct CategoryScreen {
    id: Option<ShelfId>,
    grid: PagedGrid,
    lang: Option<UiLanguage>,
    is_scroll_reset: bool,
}

impl CategoryScreen {
    pub(crate) fn seed(&mut self, id: ShelfId, items: Vec<CatalogItem>) {
        let source = if id.is_remote() { Source::Remote } else { Source::Local };

        self.id = Some(id);
        self.grid = PagedGrid::seeded(items, source);
        self.is_scroll_reset = true;
    }

    pub(crate) fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme, id: ShelfId) -> Option<NavAction> {
        if id.is_remote() {
            if let Some(problem) = gate::tmdb_key_problem(&svc.settings) {
                return gate::tmdb_key(ui, theme, problem);
            }
        }

        if self.id != Some(id) {
            self.seed(id, Vec::new());
        }

        let lang = svc.settings.general.language;
        if self.lang.is_some_and(|seen| seen != lang) && id.is_remote() {
            self.grid.reset();
        }
        self.lang = Some(lang);

        let header = |header_ui: &mut Ui| {
            header_ui.add_space(8.0);
            header_ui.label(
                RichText::new(crate::i18n::shelf_title(id).as_ref())
                    .font(theme.title_font(theme.text_heading))
                    .color(theme.title),
            );
            header_ui.add_space(12.0);
        };

        let is_scroll_reset = std::mem::take(&mut self.is_scroll_reset);
        let view = GridView { scroll_salt: ("category-page", id.as_key()), is_scroll_reset, header };
        let out = self.grid.ui(ui, svc, theme, view);

        if out.is_page_wanted {
            let tmdb = jobs::TmdbCtx::from(&svc.settings);
            self.grid.request(move |page| jobs::load_catalog_page(tmdb, id, page));
        }

        out.action
    }
}

impl super::LiveTmdb for CategoryScreen {
    fn forget_live(&mut self) {
        self.lang = None;
        if self.id.is_some_and(ShelfId::is_remote) {
            self.grid.reset();
        }
    }
}
