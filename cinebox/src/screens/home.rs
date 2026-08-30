use cinebox_core::i18n::Msg;
use cinebox_core::{HomeCatalog, HomeRow};
use egui::{RichText, Ui};
use egui_async::Bind;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, poster, scroll};

pub struct HomeScreen {
    catalog: Bind<HomeCatalog, String>,
}

impl Default for HomeScreen {
    fn default() -> Self {
        Self {
            catalog: Bind::new(true),
        }
    }
}

impl HomeScreen {
    pub fn refresh(&mut self) {
        self.catalog.clear();
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        if svc.settings.tmdb.api_key.is_empty() {
            ui.label(RichText::new(Msg::NeedTmdbKey.en()).color(theme.muted));
            if ui.button(Msg::NavSettings.en()).clicked() {
                return Some(NavAction::OpenSettings);
            }
            return None;
        }

        let settings = svc.settings.clone();
        let mut action = None;
        match self
            .catalog
            .read_or_request(move || jobs::load_home(settings))
        {
            None => widgets::page_spinner(ui, theme),
            Some(Err(error)) => {
                ui.label(RichText::new(error).color(theme.err));
                if ui.button("Retry").clicked() {
                    self.catalog.clear();
                }
            }
            Some(Ok(catalog)) => {
                queue_home_posters(svc, catalog);
                action = catalog_view(ui, catalog, svc, theme);
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
            ui.label(RichText::new(Msg::EmptyRow.en()).size(13.0).color(theme.muted));
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
