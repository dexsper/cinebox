use cinebox_core::i18n::Msg;
use cinebox_core::{PersonDetails, TmdbId, tmdb_image_url, typograph};
use egui::{RichText, Ui, Vec2};
use egui_async::Bind;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{poster, scroll};

pub struct PersonScreen {
    id: Option<TmdbId>,
    bind: Bind<Box<PersonDetails>, String>,
}

impl Default for PersonScreen {
    fn default() -> Self {
        Self {
            id: None,
            bind: Bind::new(true),
        }
    }
}

impl PersonScreen {
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        id: TmdbId,
    ) -> Option<NavAction> {
        if self.id != Some(id) {
            self.id = Some(id);
            self.bind = Bind::new(true);
        }
        let settings = svc.settings.clone();
        let mut action = None;
        match self
            .bind
            .read_or_request(move || jobs::load_person(settings, id))
        {
            None => {
                ui.spinner();
                ui.label(RichText::new(Msg::LoadingCard.en()).color(theme.muted));
            }
            Some(Err(error)) => {
                ui.label(RichText::new(error).color(theme.err));
                if ui.button("Retry").clicked() {
                    self.bind.clear();
                }
            }
            Some(Ok(details)) => {
                let proxy = svc.settings.interface.use_system_proxy;
                let size = svc.settings.tmdb.poster_size;
                if let Some(url) = tmdb_image_url(details.profile_path.as_deref(), "w185") {
                    svc.images.request(url, false, proxy);
                }
                for item in &details.credits {
                    svc.images.request_poster(item, size, proxy);
                }
                action = ready(ui, details, svc, theme);
            }
        }
        action
    }
}

fn ready(
    ui: &mut Ui,
    details: &PersonDetails,
    svc: &Services,
    theme: &Theme,
) -> Option<NavAction> {
    let mut action = None;
    scroll::vertical(ui, "person-page", |ui| {
        ui.horizontal(|ui| {
            let url = tmdb_image_url(details.profile_path.as_deref(), "w185");
            let tex = url.as_deref().and_then(|u| svc.images.get(u));
            poster::rounded_image(ui, tex, Vec2::new(140.0, 210.0), theme);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(typograph(&details.name))
                        .size(26.0)
                        .color(theme.title),
                );
                if let Some(born) = details.birthday.as_deref() {
                    ui.label(RichText::new(born).size(13.0).color(theme.muted));
                }
                if let Some(place) = details.place_of_birth.as_deref() {
                    ui.label(RichText::new(typograph(place)).size(13.0).color(theme.muted));
                }
            });
        });
        if let Some(bio) = details.biography.as_deref() {
            ui.add_space(12.0);
            ui.label(RichText::new(typograph(bio)).size(14.0).color(theme.body));
        }
        if !details.credits.is_empty() {
            ui.add_space(12.0);
            ui.label(RichText::new(Msg::Credits.en()).size(16.0).color(theme.title));
            ui.horizontal_wrapped(|ui| {
                for item in &details.credits {
                    let tex = svc.images.poster(item, svc.settings.tmdb.poster_size);
                    if let Some(nav) = poster::catalog_tile(ui, item, tex, theme) {
                        action = Some(nav);
                    }
                }
            });
        }
    });
    action
}
