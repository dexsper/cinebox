use cinebox_core::i18n::Msg;
use cinebox_core::{
    CacheHit, CatalogItem, CreditPerson, KIND_MEDIA, MediaDetails, MediaKind, TmdbId, format_money,
    format_release_date, language_key, media_cache_id, media_ttl, tmdb_image_url, typograph,
};
use egui::{Align, Atom, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2, vec2};
use egui_async::Bind;
use egui_material_icons::icons::ICON_PLAY_CIRCLE;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, intro, poster, scroll, skeleton};

pub struct MediaScreen {
    kind: Option<MediaKind>,
    id: Option<TmdbId>,
    preview: Option<CatalogItem>,
    bind: Bind<Box<MediaDetails>, String>,
    disk: Option<CacheHit<Box<MediaDetails>>>,
    intro_at: Option<f64>,
    pending_intro: bool,
    force_refresh: bool,
}

impl Default for MediaScreen {
    fn default() -> Self {
        Self {
            kind: None,
            id: None,
            preview: None,
            bind: Bind::new(true),
            disk: None,
            intro_at: None,
            pending_intro: false,
            force_refresh: false,
        }
    }
}

impl MediaScreen {
    pub fn ready(&mut self) -> Option<&MediaDetails> {
        if let Some(Ok(details)) = self.bind.read() {
            return Some(details);
        }

        self.disk.as_ref().map(|hit| hit.value.as_ref())
    }

    pub fn seed(&mut self, item: CatalogItem) {
        if self.kind != Some(item.kind) || self.id != Some(item.id) {
            self.bind = Bind::new(true);
            self.disk = None;
            self.force_refresh = false;
        }
        self.kind = Some(item.kind);
        self.id = Some(item.id);
        self.preview = Some(item);
        self.pending_intro = true;
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        kind: MediaKind,
        id: TmdbId,
    ) -> Option<NavAction> {
        let now = ui.input(|i| i.time);
        if self.kind != Some(kind) || self.id != Some(id) {
            self.kind = Some(kind);
            self.id = Some(id);
            self.bind = Bind::new(true);
            self.disk = None;
            self.force_refresh = false;
            if self
                .preview
                .as_ref()
                .is_none_or(|item| item.kind != kind || item.id != id)
            {
                self.preview = None;
            }
        }
        if self.disk.is_none() {
            let lang = language_key(svc.settings.tmdb.data_language.as_deref());
            let cache_id = media_cache_id(kind, id);
            self.disk = svc.db.as_ref().and_then(|db| {
                db.get_json::<MediaDetails>(lang, KIND_MEDIA, &cache_id)
                    .ok()
                    .flatten()
                    .map(|hit| CacheHit {
                        value: Box::new(hit.value),
                        fetched_at: hit.fetched_at,
                    })
            });
        }
        self.start_intro_if_pending(now);

        let t = intro::t(self.intro_at, now);
        if intro::running(self.intro_at, now) {
            ui.ctx().request_repaint();
        }

        if let Some(item) = &self.preview {
            svc.images.request_poster(
                item,
                svc.settings.tmdb.poster_size,
                svc.settings.interface.use_system_proxy,
            );
        }

        let settings = svc.settings.clone();
        let db = svc.db.clone();
        let skip_network = !self.force_refresh
            && self
                .disk
                .as_ref()
                .is_some_and(|hit| hit.is_fresh(media_ttl(&hit.value)));
        let has_disk = self.disk.is_some();
        let outcome = super::swr::resolve(&mut self.bind, has_disk, skip_network, move || {
            jobs::load_media(settings, kind, id, db)
        });
        if outcome.from_network {
            self.force_refresh = false;
        }
        if outcome.in_flight {
            ui.ctx().request_repaint();
        }

        let mut retry = false;
        let action = match outcome.view {
            super::swr::Swr::Live => match self.bind.read() {
                Some(Ok(details)) => {
                    queue_media_assets(svc, details);
                    ready(ui, details, svc, theme, t)
                }
                _ => None,
            },
            super::swr::Swr::Disk => match self.disk.as_ref() {
                Some(hit) => {
                    queue_media_assets(svc, &hit.value);
                    ready(ui, &hit.value, svc, theme, t)
                }
                None => None,
            },
            super::swr::Swr::Failed => {
                if let Some(Err(error)) = self.bind.read() {
                    ui.label(RichText::new(error).color(theme.err));
                }
                retry = ui.button("Retry").clicked();
                None
            }
            super::swr::Swr::Pending => {
                if let Some(item) = self.preview.as_ref() {
                    loading(ui, svc, theme, t, item);
                } else {
                    widgets::page_spinner(ui, theme);
                }
                None
            }
        };
        if retry {
            self.bind.clear();
            self.force_refresh = true;
        }

        action
    }

    fn start_intro_if_pending(&mut self, now: f64) {
        if self.pending_intro {
            self.intro_at = Some(now);
            self.pending_intro = false;
        }
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

    let extras = details
        .collection
        .iter()
        .chain(&details.recommendations)
        .chain(&details.similar);

    for extra in extras {
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
    t: f32,
) -> Option<NavAction> {
    let mut action = None;
    let poster_size = Vec2::new(
        intro::lerp(theme.tile_w, theme.poster_w, t),
        intro::lerp(theme.tile_h, theme.poster_h, t),
    );
    let title_size = intro::lerp(theme.text_small, theme.text_hero, t);
    let year_size = intro::lerp(theme.text_caption, theme.text_section, t);
    let head = details.head_line();
    scroll::vertical(ui, "media-page", |ui| {
        ui.add_space(12.0);
        hero(
            ui,
            svc,
            theme,
            Hero {
                kind: details.kind,
                id: details.id,
                poster_path: details.poster_path.as_deref(),
                title: &details.title,
                head: &head,
                poster_size,
                title_size,
                year_size,
            },
            |ui, col_top| {
                if let Some(tagline) = details.tagline.as_deref() {
                    ui.label(
                        RichText::new(typograph(tagline))
                            .size(theme.text_subtitle)
                            .color(theme.muted),
                    );
                }
                let has_rating = details.vote.filter(|v| *v > 0.0).is_some()
                    || details
                        .certification
                        .as_deref()
                        .is_some_and(|s| !s.is_empty());
                if has_rating {
                    ui.add_space(14.0);
                }
                ratings_row(ui, details, theme);
                let bits = details.detail_bits();
                if !bits.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(bits.join(" · "))
                            .size(theme.text_section)
                            .color(theme.title),
                    );
                }
                let used = ui.cursor().top() - col_top;
                let btn_h = 48.0;
                let gap = (poster_size.y - used - btn_h).max(12.0);
                ui.add_space(gap);
                if watch_button(ui, theme) {
                    action = Some(NavAction::WatchTorrents);
                }
            },
        );

        if let Some(overview) = details.overview.as_deref() {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_max_width(theme.overview_max_w);
                ui.label(
                    RichText::new(Msg::InDetail.en())
                        .font(theme.title_font(theme.text_section))
                        .color(theme.title),
                );
                ui.label(
                    RichText::new(typograph(overview))
                        .size(theme.text_label)
                        .color(theme.body),
                );
            });
        }

        ui.add_space(28.0);
        facts(ui, details, theme);
        if !details.directors.is_empty() {
            people(
                ui,
                Msg::Directors.en(),
                &details.directors,
                svc,
                theme,
                &mut action,
            );
        }

        if !details.cast.is_empty() {
            people(ui, Msg::Cast.en(), &details.cast, svc, theme, &mut action);
        }
        shelf(
            ui,
            Msg::Collection.en(),
            &details.collection,
            svc,
            theme,
            &mut action,
        );
        shelf(
            ui,
            Msg::Recommendations.en(),
            &details.recommendations,
            svc,
            theme,
            &mut action,
        );
        shelf(
            ui,
            Msg::Similar.en(),
            &details.similar,
            svc,
            theme,
            &mut action,
        );
        if !details.trailers.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(Msg::Trailers.en())
                    .font(theme.title_font(theme.text_section))
                    .color(theme.title),
            );
            for trailer in &details.trailers {
                if ui.button(typograph(&trailer.name)).clicked() {
                    action = Some(NavAction::OpenUrl(trailer.watch_url()));
                }
            }
        }
    });
    action
}

fn loading(ui: &mut Ui, svc: &Services, theme: &Theme, t: f32, item: &CatalogItem) {
    let poster_size = Vec2::new(
        intro::lerp(theme.tile_w, theme.poster_w, t),
        intro::lerp(theme.tile_h, theme.poster_h, t),
    );
    let title_size = intro::lerp(theme.text_small, theme.text_hero, t);
    let year_size = intro::lerp(theme.text_caption, theme.text_section, t);
    let year = item
        .year
        .map(|year| year.to_string())
        .unwrap_or_else(|| String::from("—"));
    let pulse = skeleton::pulse(ui);
    scroll::vertical(ui, "media-page", |ui| {
        ui.add_space(12.0);
        hero(
            ui,
            svc,
            theme,
            Hero {
                kind: item.kind,
                id: item.id,
                poster_path: item.poster_path.as_deref(),
                title: &item.title,
                head: &year,
                poster_size,
                title_size,
                year_size,
            },
            |ui, col_top| {
                ui.add_space(8.0);
                skeleton::bar(ui, theme, 280.0, 16.0, pulse);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    skeleton::bar(ui, theme, 72.0, 32.0, pulse);
                    skeleton::bar(ui, theme, 48.0, 32.0, pulse);
                });
                ui.add_space(10.0);
                skeleton::bar(ui, theme, 220.0, 14.0, pulse);
                let used = ui.cursor().top() - col_top;
                let gap = (poster_size.y - used - 46.0).max(12.0);
                ui.add_space(gap);
                skeleton::bar(ui, theme, 176.0, 46.0, pulse);
            },
        );
        ui.add_space(16.0);
        skeleton::bar(ui, theme, 120.0, 16.0, pulse);
        ui.add_space(8.0);
        skeleton::bar(
            ui,
            theme,
            theme.overview_max_w.min(ui.available_width()),
            14.0,
            pulse,
        );
        ui.add_space(6.0);
        skeleton::bar(
            ui,
            theme,
            theme.overview_max_w.min(ui.available_width()) * 0.85,
            14.0,
            pulse,
        );
        ui.add_space(6.0);
        skeleton::bar(
            ui,
            theme,
            theme.overview_max_w.min(ui.available_width()) * 0.7,
            14.0,
            pulse,
        );
        ui.add_space(28.0);
        ui.horizontal(|ui| {
            skeleton::bar(ui, theme, 88.0, 36.0, pulse);
            ui.add_space(24.0);
            skeleton::bar(ui, theme, 88.0, 36.0, pulse);
            ui.add_space(24.0);
            skeleton::bar(ui, theme, 120.0, 36.0, pulse);
        });
        ui.add_space(16.0);
        skeleton::bar(ui, theme, 80.0, 16.0, pulse);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for _ in 0..5 {
                skeleton::poster(ui, theme, vec2(100.0, 150.0), pulse);
            }
        });
    });
}

struct Hero<'a> {
    kind: MediaKind,
    id: TmdbId,
    poster_path: Option<&'a str>,
    title: &'a str,
    head: &'a str,
    poster_size: Vec2,
    title_size: f32,
    year_size: f32,
}

fn hero(
    ui: &mut Ui,
    svc: &Services,
    theme: &Theme,
    hero: Hero<'_>,
    meta: impl FnOnce(&mut Ui, f32),
) {
    ui.horizontal(|ui| {
        let tex = svc.images.poster_key(
            hero.kind,
            hero.id,
            hero.poster_path,
            svc.settings.tmdb.poster_size,
        );
        poster::rounded_image(ui, tex, hero.poster_size, theme);
        ui.add_space(28.0);
        let col_w = ui.available_width();
        ui.vertical(|ui| {
            ui.set_width(col_w);
            let col_top = ui.cursor().top();
            if !hero.head.is_empty() {
                ui.label(
                    RichText::new(hero.head)
                        .size(hero.year_size)
                        .color(theme.muted),
                );
                ui.add_space(6.0);
            }
            ui.label(
                RichText::new(typograph(hero.title))
                    .font(theme.title_font(hero.title_size))
                    .color(theme.title),
            );
            meta(ui, col_top);
        });
    });
}

fn ratings_row(ui: &mut Ui, details: &MediaDetails, theme: &Theme) {
    let vote = details.vote.filter(|v| *v > 0.0);
    let cert = details.certification.as_deref().filter(|s| !s.is_empty());
    if vote.is_none() && cert.is_none() {
        return;
    }
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if let Some(vote) = vote {
            pill(ui, theme, |ui| {
                let score = format!("{vote:.1}");
                ui.label(RichText::new(score).size(theme.text_subtitle).color(theme.rate));
                ui.label(RichText::new("TMDB").size(theme.text_caption).color(theme.muted));
            });
        }
        if let Some(cert) = cert {
            pill(ui, theme, |ui| {
                ui.label(RichText::new(cert).size(theme.text_subtitle).color(theme.title));
            });
        }
    });
}

fn pill(ui: &mut Ui, theme: &Theme, add: impl FnOnce(&mut Ui)) {
    const INNER_H: f32 = 22.0;

    Frame::new()
        .fill(theme.rating_pill)
        .corner_radius(6)
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.set_min_height(INNER_H);
            ui.set_max_height(INNER_H);
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.set_min_height(INNER_H);
                ui.set_max_height(INNER_H);
                add(ui);
            });
        });
}

fn watch_button(ui: &mut Ui, theme: &Theme) -> bool {
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.bg_fill = theme.btn_primary_bg;
        ui.visuals_mut().widgets.hovered.bg_fill = theme.btn_primary_hover;
        ui.visuals_mut().widgets.active.bg_fill = theme.btn_primary_hover;
        ui.add(
            egui::Button::new((
                Atom::grow(),
                ICON_PLAY_CIRCLE
                    .rich_text()
                    .size(theme.text_cta_icon)
                    .color(theme.btn_primary_fg),
                RichText::new(Msg::WatchTorrents.en())
                    .font(theme.emphasis_font(theme.text_subtitle))
                    .color(theme.btn_primary_fg),
                Atom::grow(),
            ))
            .fill(theme.btn_primary_bg)
            .stroke(Stroke::NONE)
            .gap(8.0)
            .corner_radius(theme.rounding(theme.radius_card))
            .min_size(vec2(176.0, 46.0)),
        )
        .clicked()
    })
    .inner
}

fn facts(ui: &mut Ui, details: &MediaDetails, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        let release = details
            .released
            .as_deref()
            .map(format_release_date)
            .or_else(|| details.year.map(|year| year.to_string()));

        if let Some(release) = release {
            fact(ui, Msg::Release.en(), release, theme);
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
        ui.label(RichText::new(label).size(theme.text_small).color(theme.muted));
        ui.label(RichText::new(value).size(theme.text_section).color(theme.title));
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
    ui.label(
        RichText::new(title)
            .font(theme.title_font(theme.text_section))
            .color(theme.title),
    );
    const TILE_W: f32 = 100.0;
    const PHOTO: Vec2 = vec2(100.0, 150.0);
    const CAPTION_GAP: f32 = 4.0;
    const LINE_GAP: f32 = 2.0;

    let name_size = theme.text_caption;
    let role_size = theme.text_micro;
    let pad = theme.ring_pad();
    let (name_slot, role_slot) = ui.ctx().fonts_mut(|f| {
        (
            f.row_height(&theme.ui_font(name_size)) * 2.0,
            f.row_height(&theme.ui_font(role_size)) * 2.0,
        )
    });

    let well_w = TILE_W + pad * 2.0;
    let tile_h = pad + PHOTO.y + CAPTION_GAP + name_slot + LINE_GAP + role_slot;
    scroll::horizontal(ui, title.to_owned(), |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for person in people {
                let url = tmdb_image_url(person.profile_path.as_deref(), "w185");
                let tex = svc.images.slot(url.as_deref());
                let name = poster::wrap_lines(
                    ui,
                    &typograph(&person.name),
                    theme.title,
                    name_size,
                    TILE_W,
                    2,
                    theme,
                );
                let role = poster::wrap_lines(
                    ui,
                    &typograph(&person.role),
                    theme.muted,
                    role_size,
                    TILE_W,
                    2,
                    theme,
                );
                let name_h = name.size().y;
                let (rect, response) = ui.allocate_exact_size(vec2(well_w, tile_h), Sense::click());
                let photo_rect = Rect::from_min_size(rect.min + vec2(pad, pad), PHOTO);
                poster::paint_poster(ui, photo_rect, tex, theme);
                if response.hovered() {
                    poster::hover_ring(ui, photo_rect, theme);
                }

                let name_pos = pos2(photo_rect.left(), photo_rect.bottom() + CAPTION_GAP);
                ui.painter().galley(name_pos, name, theme.title);
                ui.painter().galley(
                    name_pos + vec2(0.0, name_h + LINE_GAP),
                    role,
                    theme.muted,
                );
                if response.clicked() {
                    *action = Some(NavAction::OpenPerson {
                        person: person.clone(),
                    });
                }
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
    ui.label(
        RichText::new(title)
            .font(theme.title_font(theme.text_section))
            .color(theme.title),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_item() -> CatalogItem {
        CatalogItem {
            id: TmdbId::new(1),
            kind: MediaKind::Movie,
            title: "Dune".into(),
            year: Some(2021),
            vote: Some(8.0),
            poster_path: Some("/dune.jpg".into()),
        }
    }

    #[test]
    fn seed_starts_intro_once() {
        let mut screen = MediaScreen::default();
        screen.seed(sample_item());
        screen.start_intro_if_pending(1.0);
        assert!(intro::running(screen.intro_at, 1.05));

        screen.start_intro_if_pending(1.1);
        assert!((intro::t(screen.intro_at, 1.1) - intro::t(Some(1.0), 1.1)).abs() < f32::EPSILON);
    }

    #[test]
    fn returning_without_seed_does_not_replay() {
        let mut screen = MediaScreen::default();
        screen.seed(sample_item());
        screen.start_intro_if_pending(0.0);
        assert!(!intro::running(screen.intro_at, 1.0));
        screen.start_intro_if_pending(10.0);
        assert!(!intro::running(screen.intro_at, 10.0));
    }
}
