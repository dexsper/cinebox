use cinebox_core::i18n::Msg;
use cinebox_core::{
    CacheHit, CatalogItem, CreditPerson, KIND_MEDIA, MediaDetails, MediaKind, TmdbId, UiLanguage,
    format_money, format_release_date, language_key, media_cache_id, media_ttl, tmdb_image_url,
};
use egui::{Atom, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};
use egui_async::Bind;
use egui_material_icons::icons::ICON_PLAY_CIRCLE;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::{Services, db_block_on};
use crate::theme::Theme;
use crate::widgets::{self, intro, poster, scroll, skeleton};

const WATCH_BTN_SIZE: Vec2 = vec2(176.0, 46.0);

pub struct MediaScreen {
    kind: Option<MediaKind>,
    id: Option<TmdbId>,
    preview: Option<CatalogItem>,
    bind: Bind<Box<MediaDetails>, String>,
    disk: Option<CacheHit<Box<MediaDetails>>>,
    intro_at: Option<f64>,
    pending_intro: bool,
    force_refresh: bool,
    lang: Option<UiLanguage>,
    reset_scroll: bool,
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
            lang: None,
            reset_scroll: false,
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
        self.reset_scroll = true;
    }

    /// Drop the in-memory card so the next paint reloads for the new language.
    /// Does not touch the SQLite cache.
    pub fn forget_live(&mut self) {
        self.lang = None;
        self.bind = Bind::new(true);
        self.disk = None;
        self.force_refresh = true;
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
            self.reset_scroll = true;

            let is_different = self
                .preview
                .as_ref()
                .is_none_or(|item| item.kind != kind || item.id != id);

            if is_different {
                self.preview = None;
            }
        }

        let lang = svc.settings.general.language;
        if self.lang != Some(lang) {
            let switched = self.lang.is_some();
            self.lang = Some(lang);
            if switched {
                self.bind = Bind::new(true);
                self.disk = None;
                self.force_refresh = true;
            }
        }

        if self.disk.is_none() {
            let lang = language_key(Some(svc.settings.general.language.tmdb_code()));
            let cache_id = media_cache_id(kind, id);
            self.disk = svc.db.as_ref().and_then(|db| {
                db_block_on(db.get_json::<MediaDetails>(lang, KIND_MEDIA, &cache_id))
                    .ok()
                    .flatten()
                    .map(|hit| {
                        let mut value = hit.value;
                        value.apply_typography();
                        CacheHit {
                            value: Box::new(value),
                            fetched_at: hit.fetched_at,
                        }
                    })
            });
        }
        self.start_intro_if_pending(now);

        let t = intro::t(self.intro_at, now);
        if intro::running(self.intro_at, now) {
            ui.ctx().request_repaint();
        }

        let settings = svc.settings.clone();
        let db = svc.db.clone();
        let cache = self.disk.as_ref();
        let fresh = cache.is_some_and(|hit| hit.is_fresh(media_ttl(&hit.value)));
        let skip_network = !self.force_refresh && fresh;

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
        let to_top = self.reset_scroll;
        let action = match outcome.view {
            super::swr::Swr::Live => match self.bind.read() {
                Some(Ok(details)) => {
                    self.reset_scroll = false;
                    ready(ui, details, svc, theme, t, to_top)
                }
                _ => None,
            },
            super::swr::Swr::Disk => match self.disk.as_ref() {
                Some(hit) => {
                    self.reset_scroll = false;
                    ready(ui, &hit.value, svc, theme, t, to_top)
                }
                None => None,
            },
            super::swr::Swr::Failed => {
                let error = match self.bind.read() {
                    Some(Err(error)) => error.clone(),
                    _ => Msg::Failed.t().to_owned(),
                };
                retry = widgets::page_error(ui, theme, &error);
                None
            }
            super::swr::Swr::Pending => {
                if let Some(item) = self.preview.as_ref() {
                    self.reset_scroll = false;
                    loading(ui, svc, theme, t, item, to_top);
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

fn ready(
    ui: &mut Ui,
    details: &MediaDetails,
    svc: &Services,
    theme: &Theme,
    t: f32,
    to_top: bool,
) -> Option<NavAction> {
    let mut action = None;
    let poster_size = Vec2::new(
        intro::lerp(theme.tile_w, theme.poster_w, t),
        intro::lerp(theme.tile_h, theme.poster_h, t),
    );

    let title_size = intro::lerp(theme.text_small, theme.text_hero, t);
    let year_size = intro::lerp(theme.text_caption, theme.text_section, t);
    let head = details.head_line();

    scroll_page(ui, details.kind, details.id, to_top, |ui| {
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
                        RichText::new(tagline)
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

                widgets::rating::row(ui, theme, details.vote, details.certification.as_deref());
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
                let gap = (poster_size.y - used - WATCH_BTN_SIZE.y).max(12.0);

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
                    RichText::new(Msg::InDetail.t())
                        .font(theme.title_font(theme.text_section))
                        .color(theme.title),
                );
                ui.label(
                    RichText::new(overview)
                        .size(theme.text_label)
                        .color(theme.body),
                );
            });
        }

        ui.add_space(28.0);
        facts(ui, details, theme);
        if !details.directors.is_empty() {
            let title = Msg::Directors.t();
            people(
                ui,
                title,
                "media-directors",
                &details.directors,
                svc,
                theme,
                &mut action,
            );
        }

        if !details.cast.is_empty() {
            let title = Msg::Cast.t();
            people(
                ui,
                title,
                "media-cast",
                &details.cast,
                svc,
                theme,
                &mut action,
            );
        }

        shelf(
            ui,
            Msg::Collection.t(),
            "media-collection",
            &details.collection,
            svc,
            theme,
            &mut action,
        );
        shelf(
            ui,
            Msg::Recommendations.t(),
            "media-recommendations",
            &details.recommendations,
            svc,
            theme,
            &mut action,
        );
        shelf(
            ui,
            Msg::Similar.t(),
            "media-similar",
            &details.similar,
            svc,
            theme,
            &mut action,
        );

        if !details.trailers.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(Msg::Trailers.t())
                    .font(theme.title_font(theme.text_section))
                    .color(theme.title),
            );

            for trailer in &details.trailers {
                if crate::widgets::button::label(
                    ui,
                    theme,
                    &trailer.name,
                    crate::widgets::button::Opts::secondary(vec2(
                        0.0,
                        crate::widgets::combo::HEIGHT,
                    )),
                ) {
                    action = Some(NavAction::OpenUrl(trailer.watch_url()));
                }
            }
        }
    });
    action
}

fn loading(ui: &mut Ui, svc: &Services, theme: &Theme, t: f32, item: &CatalogItem, to_top: bool) {
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
    scroll_page(ui, item.kind, item.id, to_top, |ui| {
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
                let gap = (poster_size.y - used - WATCH_BTN_SIZE.y).max(12.0);

                ui.add_space(gap);
                skeleton::bar(ui, theme, WATCH_BTN_SIZE.x, WATCH_BTN_SIZE.y, pulse);
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
        let poster = poster::rounded_image(ui, hero.poster_size, theme, || {
            svc.images.poster_key(
                hero.kind,
                hero.id,
                hero.poster_path,
                svc.settings.tmdb.poster_size,
            )
        });
        if svc.is_watched(hero.kind, hero.id) {
            poster::watched_badge(ui, poster, theme);
        }
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
                RichText::new(hero.title)
                    .font(theme.title_font(hero.title_size))
                    .color(theme.title),
            );
            meta(ui, col_top);
        });
    });
}

fn watch_button(ui: &mut Ui, theme: &Theme) -> bool {
    crate::widgets::button::add_named(
        ui,
        theme,
        (
            Atom::grow(),
            ICON_PLAY_CIRCLE
                .rich_text()
                .size(theme.text_cta_icon)
                .color(theme.btn_primary_fg),
            RichText::new(Msg::WatchTorrents.t())
                .font(theme.emphasis_font(theme.text_subtitle))
                .color(theme.btn_primary_fg),
            Atom::grow(),
        ),
        crate::widgets::button::Opts::primary(WATCH_BTN_SIZE),
        Some(Msg::WatchTorrents.t()),
    )
    .clicked()
}

fn facts(ui: &mut Ui, details: &MediaDetails, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        let release = details
            .released
            .as_deref()
            .map(format_release_date)
            .or_else(|| details.year.map(|year| year.to_string()));

        if let Some(release) = release {
            fact(ui, Msg::Release.t(), release, theme);
        }

        if let Some(budget) = details.budget {
            fact(ui, Msg::Budget.t(), format_money(budget), theme);
        }

        if !details.countries.is_empty() {
            fact(ui, Msg::Countries.t(), details.countries.join(", "), theme);
        }
    });
}

fn fact(ui: &mut Ui, label: &str, value: String, theme: &Theme) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(label)
                .size(theme.text_small)
                .color(theme.muted),
        );
        ui.label(
            RichText::new(value)
                .size(theme.text_section)
                .color(theme.title),
        );
    });

    ui.add_space(24.0);
}

fn people(
    ui: &mut Ui,
    title: &str,
    salt: &'static str,
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
    scroll::horizontal(ui, salt, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for person in people {
                let size = vec2(well_w, tile_h);
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());
                let response = crate::widgets::button::pointing(response);

                if !poster::in_load_window(ui, rect) {
                    if response.clicked() {
                        *action = Some(NavAction::OpenPerson {
                            person: person.clone(),
                        });
                    }

                    continue;
                }

                let url = tmdb_image_url(person.profile_path.as_deref(), "w185");
                let tex = svc.images.slot(url.as_deref());
                let name = poster::wrap_lines(
                    ui,
                    &person.name,
                    theme.title,
                    name_size,
                    TILE_W,
                    2,
                    theme,
                );

                let role = poster::wrap_lines(
                    ui,
                    &person.role,
                    theme.muted,
                    role_size,
                    TILE_W,
                    2,
                    theme,
                );

                let name_h = name.size().y;
                let photo_rect = Rect::from_min_size(rect.min + vec2(pad, pad), PHOTO);

                poster::paint_poster(ui, photo_rect, tex, theme);
                if response.hovered() {
                    poster::hover_ring(ui, photo_rect, theme);
                }

                let name_pos = pos2(photo_rect.left(), photo_rect.bottom() + CAPTION_GAP);
                ui.painter().galley(name_pos, name, theme.title);
                ui.painter()
                    .galley(name_pos + vec2(0.0, name_h + LINE_GAP), role, theme.muted);

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
    salt: &'static str,
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

    scroll::horizontal(ui, salt, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for item in items {
                if let Some(nav) = poster::catalog_tile(
                    ui,
                    item,
                    &svc.images,
                    svc.settings.tmdb.poster_size,
                    theme,
                    svc.is_watched(item.kind, item.id),
                ) {
                    *action = Some(nav);
                }
            }
        });
    });
}

fn scroll_page(
    ui: &mut Ui,
    kind: MediaKind,
    id: TmdbId,
    to_top: bool,
    add: impl FnOnce(&mut Ui),
) {
    let salt = ("media-page", kind, id);
    if to_top {
        scroll::vertical_to_top(ui, salt, add);
        return;
    }

    scroll::vertical(ui, salt, add);
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

    #[test]
    fn seed_asks_to_reset_scroll() {
        let mut screen = MediaScreen::default();
        assert!(!screen.reset_scroll);

        screen.seed(sample_item());
        assert!(screen.reset_scroll);
    }
}
