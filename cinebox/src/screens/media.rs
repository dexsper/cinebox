use cinebox_core::i18n::Msg;
use cinebox_core::{
    CatalogItem, CreditPerson, MediaDetails, MediaKind, TmdbId, format_money, format_release_date,
    tmdb_image_url, typograph,
};
use egui::{Frame, RichText, Ui, Vec2};
use egui_async::Bind;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{poster, scroll};

pub struct MediaScreen {
    kind: Option<MediaKind>,
    id: Option<TmdbId>,
    bind: Bind<Box<MediaDetails>, String>,
}

impl Default for MediaScreen {
    fn default() -> Self {
        Self {
            kind: None,
            id: None,
            bind: Bind::new(true),
        }
    }
}

impl MediaScreen {
    pub fn ready(&mut self) -> Option<&MediaDetails> {
        self.bind.read().as_ref().and_then(|r| r.as_ref().ok()).map(|b| &**b)
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        kind: MediaKind,
        id: TmdbId,
    ) -> Option<NavAction> {
        if self.kind != Some(kind) || self.id != Some(id) {
            self.kind = Some(kind);
            self.id = Some(id);
            self.bind = Bind::new(true);
        }
        let settings = svc.settings.clone();
        let mut action = None;
        match self
            .bind
            .read_or_request(move || jobs::load_media(settings, kind, id))
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
                queue_media_assets(svc, details);
                action = ready(ui, details, svc, theme);
            }
        }
        action
    }
}

fn queue_media_assets(svc: &mut Services, details: &MediaDetails) {
    let size = svc.settings.tmdb.poster_size;
    let proxy = svc.settings.interface.use_system_proxy;
    let item = CatalogItem {
        id: details.id,
        kind: details.kind,
        title: String::new(),
        year: details.year,
        vote: details.vote,
        poster_path: details.poster_path.clone(),
    };
    svc.images.request_poster(&item, size, proxy);
    for extra in details
        .collection
        .iter()
        .chain(details.recommendations.iter())
        .chain(details.similar.iter())
    {
        svc.images.request_poster(extra, size, proxy);
    }
    if let Some(url) = tmdb_image_url(details.poster_path.as_deref(), size.tmdb_path()) {
        svc.images.request(url, false, proxy);
    }
    if let Some(url) = tmdb_image_url(details.backdrop_path.as_deref(), "w1280") {
        svc.images.request(url, true, proxy);
    }
    for person in details.directors.iter().chain(details.cast.iter()) {
        if let Some(url) = tmdb_image_url(person.profile_path.as_deref(), "w185") {
            svc.images.request(url, false, proxy);
        }
    }
}

fn ready(
    ui: &mut Ui,
    details: &MediaDetails,
    svc: &Services,
    theme: &Theme,
) -> Option<NavAction> {
    let mut action = None;
    scroll::vertical(ui, "media-page", |ui| {
        ui.horizontal(|ui| {
            let tex = svc.images.poster_key(
                details.kind,
                details.id,
                details.poster_path.as_deref(),
                svc.settings.tmdb.poster_size,
            );
            poster::rounded_image(ui, tex, Vec2::new(theme.poster_w, theme.poster_h), theme);
            ui.add_space(28.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                let head = details.head_line();
                if !head.is_empty() {
                    ui.label(RichText::new(head).size(16.0).color(theme.muted));
                }
                ui.label(
                    RichText::new(typograph(&details.title))
                        .size(36.0)
                        .color(theme.title),
                );
                if let Some(tagline) = details.tagline.as_deref() {
                    ui.label(RichText::new(typograph(tagline)).size(18.0).color(theme.muted));
                }
                ratings_row(ui, details, theme);
                let bits = details.detail_bits();
                if !bits.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for (i, bit) in bits.iter().enumerate() {
                            if i > 0 {
                                ui.label(RichText::new("●").size(11.0).color(theme.muted));
                            }
                            ui.label(RichText::new(bit).size(16.0).color(theme.title));
                        }
                    });
                }
                if ui.button(Msg::WatchTorrents.en()).clicked() {
                    action = Some(NavAction::WatchTorrents);
                }
            });
        });
        if let Some(overview) = details.overview.as_deref() {
            ui.add_space(16.0);
            ui.label(RichText::new(Msg::InDetail.en()).size(16.0).color(theme.title));
            ui.label(RichText::new(typograph(overview)).size(15.0).color(theme.body));
        }
        facts(ui, details, theme);
        if !details.directors.is_empty() {
            people(ui, Msg::Directors.en(), &details.directors, svc, theme, &mut action);
        }
        if !details.cast.is_empty() {
            people(ui, Msg::Cast.en(), &details.cast, svc, theme, &mut action);
        }
        shelf(ui, Msg::Collection.en(), &details.collection, svc, theme, &mut action);
        shelf(
            ui,
            Msg::Recommendations.en(),
            &details.recommendations,
            svc,
            theme,
            &mut action,
        );
        shelf(ui, Msg::Similar.en(), &details.similar, svc, theme, &mut action);
        if !details.trailers.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(Msg::Trailers.en()).size(16.0).color(theme.title));
            for trailer in &details.trailers {
                if ui.button(typograph(&trailer.name)).clicked() {
                    action = Some(NavAction::OpenUrl(trailer.watch_url()));
                }
            }
        }
    });
    action
}

fn ratings_row(ui: &mut Ui, details: &MediaDetails, theme: &Theme) {
    let vote = details.vote.filter(|v| *v > 0.0);
    let cert = details.certification.as_deref().filter(|s| !s.is_empty());
    if vote.is_none() && cert.is_none() {
        return;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if let Some(vote) = vote {
            pill(ui, theme, |ui| {
                ui.label(RichText::new(format!("{vote:.1}")).size(20.0).color(theme.rate));
                ui.label(RichText::new("TMDB").size(13.0).color(theme.muted));
            });
        }
        if let Some(cert) = cert {
            pill(ui, theme, |ui| {
                ui.label(RichText::new(cert).size(20.0).color(theme.title));
            });
        }
    });
}

fn pill(ui: &mut Ui, theme: &Theme, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(theme.rating_pill)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.horizontal(|ui| add(ui));
        });
}

fn facts(ui: &mut Ui, details: &MediaDetails, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        if let Some(released) = details.released.as_deref() {
            fact(ui, Msg::Release.en(), format_release_date(released), theme);
        } else if let Some(year) = details.year {
            fact(ui, Msg::Release.en(), year.to_string(), theme);
        }
        if let Some(budget) = details.budget {
            fact(ui, Msg::Budget.en(), format_money(budget), theme);
        }
        if !details.countries.is_empty() {
            fact(ui, Msg::Countries.en(), details.countries.join(", "), theme);
        }
    });
}

fn fact(ui: &mut Ui, label: &str, value: String, theme: &Theme) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(13.0).color(theme.muted));
        ui.label(RichText::new(value).size(16.0).color(theme.title));
    });
    ui.add_space(24.0);
}

fn people(
    ui: &mut Ui,
    title: &str,
    people: &[CreditPerson],
    svc: &Services,
    theme: &Theme,
    action: &mut Option<NavAction>,
) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).size(16.0).color(theme.title));
    scroll::horizontal(ui, title.to_owned(), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for person in people {
                ui.vertical(|ui| {
                    let url = tmdb_image_url(person.profile_path.as_deref(), "w185");
                    let tex = url.as_deref().and_then(|u| svc.images.get(u));
                    let inner = ui.allocate_ui(Vec2::new(100.0, 200.0), |ui| {
                        poster::rounded_image(ui, tex, Vec2::new(100.0, 150.0), theme);
                        ui.label(RichText::new(typograph(&person.name)).size(12.0).color(theme.title));
                        ui.label(RichText::new(typograph(&person.role)).size(11.0).color(theme.muted));
                    });
                    if inner.response.interact(egui::Sense::click()).clicked() {
                        *action = Some(NavAction::OpenPerson { id: person.id });
                    }
                });
            }
        });
    });
}

fn shelf(
    ui: &mut Ui,
    title: &str,
    items: &[CatalogItem],
    svc: &Services,
    theme: &Theme,
    action: &mut Option<NavAction>,
) {
    if items.is_empty() {
        return;
    }
    ui.add_space(12.0);
    ui.label(RichText::new(title).size(16.0).color(theme.title));
    scroll::horizontal(ui, title.to_owned(), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for item in items {
                let tex = svc.images.poster(item, svc.settings.tmdb.poster_size);
                if let Some(nav) = poster::catalog_tile(ui, item, tex, theme) {
                    *action = Some(nav);
                }
            }
        });
    });
}
