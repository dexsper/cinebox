//! Full-shelf grid: items already on the shelf, then extra TMDB pages on scroll.

use cinebox_core::{CatalogItem, UiLanguage};
use cinebox_tmdb::ShelfId;
use egui::{RichText, Ui};

use crate::jobs;
use crate::nav::NavAction;
use crate::screens::paged::PagedGrid;
use crate::services::Services;
use crate::theme::Theme;

#[derive(Default)]
pub struct CategoryScreen {
    id: Option<ShelfId>,
    grid: PagedGrid,
    lang: Option<UiLanguage>,
    reset_scroll: bool,
}

impl CategoryScreen {
    pub fn seed(&mut self, id: ShelfId, items: Vec<CatalogItem>) {
        self.id = Some(id);
        self.grid = PagedGrid::seeded(items, id.is_remote());
        self.reset_scroll = true;
    }

    /// Drop live pages so the next paint reloads for a new TMDB language/key.
    pub fn forget_live(&mut self) {
        self.lang = None;
        if self.id.is_some_and(ShelfId::is_remote) {
            self.grid.reset();
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        id: ShelfId,
    ) -> Option<NavAction> {
        if self.id != Some(id) {
            self.seed(id, Vec::new());
        }

        let lang = svc.settings.general.language;
        if self.lang.is_some_and(|seen| seen != lang) && id.is_remote() {
            self.grid.reset();
        }
        self.lang = Some(lang);

        let to_top = std::mem::take(&mut self.reset_scroll);
        let out = self.grid.ui(ui, svc, theme, ("category-page", id.as_key()), to_top, |ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new(crate::i18n::shelf_title(id).as_ref())
                    .font(theme.title_font(theme.text_heading))
                    .color(theme.title),
            );
            ui.add_space(12.0);
        });

        if out.wants_page {
            let tmdb = jobs::TmdbCtx::from(&svc.settings);
            self.grid
                .request(move |page| jobs::load_catalog_page(tmdb, id, page));
        }

        out.action
    }
}
