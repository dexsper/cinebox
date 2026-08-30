use cinebox_core::i18n::Msg;
use cinebox_core::{HomeCatalog, HomeRow, language_key};
use egui::{RichText, Ui};
use egui_async::Bind;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, poster, scroll};

pub struct HomeScreen {
    catalog: Bind<HomeCatalog, String>,
    force_refresh: bool,
}

impl Default for HomeScreen {
    fn default() -> Self {
        Self {
            catalog: Bind::new(true),
            force_refresh: false,
        }
    }
}

impl HomeScreen {
    pub fn refresh(&mut self) {
        self.catalog.clear();
        self.force_refresh = true;
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        if svc.settings.tmdb.api_key.is_empty() {
            ui.label(RichText::new(Msg::NeedTmdbKey.en()).color(theme.muted));
            if ui.button(Msg::NavSettings.en()).clicked() {
                return Some(NavAction::OpenSettings);
            }
            return None;
        }

        let lang = language_key(svc.settings.tmdb.data_language.as_deref());
        let disk = svc
            .db
            .as_ref()
            .and_then(|db| db.home_catalog(lang).ok().flatten());
        let (disk_catalog, disk_fresh) = match disk {
            Some((catalog, fresh)) => (Some(catalog), fresh),
            None => (None, false),
        };
        let disk_catalog = disk_catalog.as_ref();

        let mut action = None;
        if matches!(self.catalog.read(), Some(Ok(_))) {
            self.force_refresh = false;
        }
        if let Some(result) = self.catalog.read() {
            match result {
                Ok(catalog) => {
                    queue_home_posters(svc, catalog);
                    action = catalog_view(ui, catalog, svc, theme);
                }
                Err(error) => {
                    if let Some(catalog) = disk_catalog {
                        queue_home_posters(svc, catalog);
                        action = catalog_view(ui, catalog, svc, theme);
                    } else {
                        let error = error.clone();
                        ui.label(RichText::new(error).color(theme.err));
                        if ui.button("Retry").clicked() {
                            self.refresh();
                        }
                    }
                }
            }
        } else if !self.force_refresh && disk_fresh {
            if let Some(catalog) = disk_catalog {
                queue_home_posters(svc, catalog);
                action = catalog_view(ui, catalog, svc, theme);
            }
        } else {
            let settings = svc.settings.clone();
            let db = svc.db.clone();
            let mut loaded_ok = false;
            match self
                .catalog
                .read_or_request(move || jobs::load_home(settings, db))
            {
                None => {
                    if let Some(catalog) = disk_catalog {
                        queue_home_posters(svc, catalog);
                        action = catalog_view(ui, catalog, svc, theme);
                    } else {
                        widgets::page_spinner(ui, theme);
                    }
                }
                Some(Ok(catalog)) => {
                    loaded_ok = true;
                    queue_home_posters(svc, catalog);
                    action = catalog_view(ui, catalog, svc, theme);
                }
                Some(Err(error)) => {
                    if let Some(catalog) = disk_catalog {
                        queue_home_posters(svc, catalog);
                        action = catalog_view(ui, catalog, svc, theme);
                    } else {
                        let error = error.clone();
                        ui.label(RichText::new(error).color(theme.err));
                        if ui.button("Retry").clicked() {
                            self.refresh();
                        }
                    }
                }
            }
            if loaded_ok {
                self.force_refresh = false;
            }
        }
        action
    }
}

fn queue_home_posters(svc: &mut Services, catalog: &HomeCatalog) {
    let size = svc.settings.tmdb.poster_size;
    let proxy = svc.settings.interface.use_system_proxy;
    for row in &catalog.rows {
        for item in &row.items {
            svc.images.request_poster(item, size, proxy);
        }
    }
}

fn catalog_view(
    ui: &mut Ui,
    catalog: &HomeCatalog,
    svc: &Services,
    theme: &Theme,
) -> Option<NavAction> {
    let mut action = None;
    scroll::vertical(ui, "home-page", |ui| {
        for (index, row) in catalog.rows.iter().enumerate() {
            if let Some(nav) = shelf(ui, row, index, svc, theme) {
                action = Some(nav);
            }
        }
    });
    action
}

fn shelf(
    ui: &mut Ui,
    row: &HomeRow,
    index: usize,
    svc: &Services,
    theme: &Theme,
) -> Option<NavAction> {
    ui.add_space(12.0);
    ui.label(RichText::new(row.id.title()).size(16.0).color(theme.title));
    if let Some(error) = &row.error {
        ui.label(RichText::new(error).size(13.0).color(theme.err));
    }
    if row.items.is_empty() {
        if row.error.is_none() {
            ui.label(
                RichText::new(Msg::EmptyRow.en())
                    .size(13.0)
                    .color(theme.muted),
            );
        }
        return None;
    }
    let mut action = None;
    scroll::horizontal(ui, format!("home-row-{index}"), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for item in &row.items {
                let tex = svc.images.poster(item, svc.settings.tmdb.poster_size);
                if let Some(nav) = poster::catalog_tile(ui, item, tex, theme) {
                    action = Some(nav);
                }
            }
        });
    });
    action
}
