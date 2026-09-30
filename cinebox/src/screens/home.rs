use cinebox_core::{
    CatalogItem, HomeCatalog, HomeRowId, LibraryList, ListStatus, RECENT_ROW_LIMIT, language_key,
};
use cinebox_tmdb::ShelfId;
use egui::Ui;

use super::gate;
use super::shelf::shelf;
use crate::errors::UserError;
use crate::widgets::page_state::{ErrorChoice, error_page};
use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, scroll};

#[derive(Default)]
pub struct HomeScreen {
    cache: super::swr::Cached<HomeCatalog, (HomeCatalog, bool)>,
}

impl HomeScreen {
    pub fn refresh(&mut self) {
        self.cache.invalidate();
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        if let Some(problem) = gate::tmdb_key_problem(&svc.settings) {
            return gate::tmdb_key(ui, theme, problem);
        }

        self.cache.sync_lang(svc.settings.general.language);

        let lang_key = language_key(Some(svc.settings.general.language.tmdb_code())).to_owned();
        let db = svc.db.clone();
        let hydrated = self.cache.hydrate(async move {
            let db = db?;

            db.home_catalog(&lang_key).await.ok().flatten()
        });

        if !hydrated {
            widgets::page_spinner(ui, theme);
            return None;
        }

        let disk_fresh = self.cache.disk.as_ref().is_some_and(|(_, fresh)| *fresh);
        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        let db = svc.db.clone();
        let outcome = self
            .cache
            .resolve(disk_fresh, move || jobs::load_home(tmdb, db));

        let mut retry = false;
        let action = match outcome.view {
            super::swr::Swr::Live => match self.cache.bind.read() {
                Some(Ok(catalog)) => catalog_view(ui, catalog, svc, theme),
                _ => None,
            },
            super::swr::Swr::Disk => match self.cache.disk.as_ref() {
                Some((catalog, _)) => catalog_view(ui, catalog, svc, theme),
                None => None,
            },
            super::swr::Swr::Failed => {
                let error = UserError::from_read(self.cache.bind.read());
                let choice = error_page(ui, theme, &error);
                retry = choice == Some(ErrorChoice::Retry);
                choice.and_then(ErrorChoice::nav)
            }
            super::swr::Swr::Pending => {
                widgets::page_spinner(ui, theme);
                None
            }
        };
        if retry {
            self.refresh();
        }

        action
    }
}

impl super::LiveTmdb for HomeScreen {
    fn forget_live(&mut self) {
        self.refresh();
    }
}

fn catalog_view(
    ui: &mut Ui,
    catalog: &HomeCatalog,
    svc: &Services,
    theme: &Theme,
) -> Option<NavAction> {
    let hidden = &svc.settings.general.hidden_home_rows;
    let mut action = None;
    scroll::vertical(ui, "home-page", |ui| {
        for row in &catalog.rows {
            if hidden.contains(&row.id) {
                continue;
            }

            let nav = match row.id {
                HomeRowId::Watching => library_shelf(ui, row.id, ListStatus::Watching, svc, theme),
                HomeRowId::Planned => library_shelf(ui, row.id, ListStatus::Planned, svc, theme),
                id if !id.is_remote() && row.items.is_empty() => continue,
                id => shelf(ui, ShelfId::Home(id), &row.items, row.error.as_deref(), svc, theme),
            };

            if nav.is_some() {
                action = nav;
            }
        }
    });
    action
}

/// List shelves read the live library so a status change shows without reloading Home.
fn library_shelf(
    ui: &mut Ui,
    id: HomeRowId,
    status: ListStatus,
    svc: &Services,
    theme: &Theme,
) -> Option<NavAction> {
    let items: Vec<CatalogItem> = svc
        .library
        .list(LibraryList::Status(status), None)
        .into_iter()
        .take(RECENT_ROW_LIMIT)
        .map(|entry| entry.item.clone())
        .collect();

    if items.is_empty() {
        return None;
    }

    shelf(ui, ShelfId::Home(id), &items, None, svc, theme)
}
