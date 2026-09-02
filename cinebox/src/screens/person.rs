use cinebox_core::i18n::Msg;
use cinebox_core::{
    CacheHit, CreditPerson, DETAILS_TTL, KIND_PERSON, PersonDetails, TmdbId, UiLanguage,
    language_key, person_cache_id, tmdb_image_url,
};
use egui::{RichText, Ui, Vec2};
use egui_async::Bind;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::{Services, db_block_on};
use crate::theme::Theme;
use crate::widgets::{self, intro, poster, scroll, skeleton};

const FROM_W: f32 = 100.0;
const FROM_H: f32 = 150.0;
const TO_W: f32 = 140.0;
const TO_H: f32 = 210.0;

pub struct PersonScreen {
    id: Option<TmdbId>,
    preview: Option<CreditPerson>,
    bind: Bind<Box<PersonDetails>, String>,
    disk: Option<CacheHit<Box<PersonDetails>>>,
    intro_at: Option<f64>,
    pending_intro: bool,
    force_refresh: bool,
    lang: Option<UiLanguage>,
    reset_scroll: bool,
}

impl Default for PersonScreen {
    fn default() -> Self {
        Self {
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

impl PersonScreen {
    pub fn seed(&mut self, person: CreditPerson) {
        if self.id != Some(person.id) {
            self.bind = Bind::new(true);
            self.disk = None;
            self.force_refresh = false;
        }
        self.id = Some(person.id);
        self.preview = Some(person);
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
        id: TmdbId,
    ) -> Option<NavAction> {
        let now = ui.input(|i| i.time);
        if self.id != Some(id) {
            self.id = Some(id);
            self.bind = Bind::new(true);
            self.disk = None;
            self.force_refresh = false;
            self.reset_scroll = true;
            if self.preview.as_ref().is_none_or(|person| person.id != id) {
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
            let cache_id = person_cache_id(id);
            self.disk = svc.db.as_ref().and_then(|db| {
                db_block_on(db.get_json::<PersonDetails>(lang, KIND_PERSON, &cache_id))
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

        if intro::running(self.intro_at, now) {
            ui.ctx().request_repaint();
        }

        let t = intro::t(self.intro_at, now);
        let settings = svc.settings.clone();
        let db = svc.db.clone();
        let skip_network = !self.force_refresh
            && self
                .disk
                .as_ref()
                .is_some_and(|hit| hit.is_fresh(DETAILS_TTL));
        let has_disk = self.disk.is_some();
        let outcome = super::swr::resolve(&mut self.bind, has_disk, skip_network, move || {
            jobs::load_person(settings, id, db)
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
                if let Some(person) = self.preview.as_ref() {
                    self.reset_scroll = false;
                    loading(ui, svc, theme, t, person, to_top);
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
    details: &PersonDetails,
    svc: &Services,
    theme: &Theme,
    t: f32,
    to_top: bool,
) -> Option<NavAction> {
    let mut action = None;
    let photo = Vec2::new(intro::lerp(FROM_W, TO_W, t), intro::lerp(FROM_H, TO_H, t));
    let name_size = intro::lerp(theme.text_caption, theme.text_person, t);

    scroll_page(ui, details.id, to_top, |ui| {
        hero(
            ui,
            svc,
            theme,
            Hero {
                profile_path: details.profile_path.as_deref(),
                name: &details.name,
                photo,
                name_size,
            },
            |ui| {
                if let Some(born) = details.birthday.as_deref() {
                    ui.label(
                        RichText::new(born)
                            .size(theme.text_small)
                            .color(theme.muted),
                    );
                }
                if let Some(place) = details.place_of_birth.as_deref() {
                    ui.label(
                        RichText::new(place)
                            .size(theme.text_small)
                            .color(theme.muted),
                    );
                }
            },
        );

        if let Some(bio) = details.biography.as_deref() {
            ui.add_space(12.0);
            ui.label(RichText::new(bio).size(theme.text_body).color(theme.body));
        }

        if !details.credits.is_empty() {
            ui.add_space(12.0);
            ui.label(
                RichText::new(Msg::Credits.t())
                    .font(theme.title_font(theme.text_section))
                    .color(theme.title),
            );
            ui.horizontal_wrapped(|ui| {
                for item in &details.credits {
                    if let Some(nav) = poster::catalog_tile(
                        ui,
                        item,
                        &svc.images,
                        svc.settings.tmdb.poster_size,
                        theme,
                        svc.is_watched(item.kind, item.id),
                    ) {
                        action = Some(nav);
                    }
                }
            });
        }
    });
    action
}

fn loading(
    ui: &mut Ui,
    svc: &Services,
    theme: &Theme,
    t: f32,
    person: &CreditPerson,
    to_top: bool,
) {
    let photo = Vec2::new(intro::lerp(FROM_W, TO_W, t), intro::lerp(FROM_H, TO_H, t));
    let name_size = intro::lerp(theme.text_caption, theme.text_person, t);
    let pulse = skeleton::pulse(ui);
    scroll_page(ui, person.id, to_top, |ui| {
        hero(
            ui,
            svc,
            theme,
            Hero {
                profile_path: person.profile_path.as_deref(),
                name: &person.name,
                photo,
                name_size,
            },
            |_| {},
        );
        ui.add_space(8.0);
        skeleton::bar(ui, theme, 160.0, 13.0, pulse);
        ui.add_space(6.0);
        skeleton::bar(ui, theme, 220.0, 13.0, pulse);
        ui.add_space(16.0);
        skeleton::bar(
            ui,
            theme,
            ui.available_width().min(theme.overview_max_w),
            14.0,
            pulse,
        );
        ui.add_space(6.0);
        skeleton::bar(
            ui,
            theme,
            ui.available_width().min(theme.overview_max_w) * 0.9,
            14.0,
            pulse,
        );
        ui.add_space(6.0);
        skeleton::bar(
            ui,
            theme,
            ui.available_width().min(theme.overview_max_w) * 0.65,
            14.0,
            pulse,
        );
        ui.add_space(16.0);
        skeleton::bar(ui, theme, 100.0, 16.0, pulse);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for _ in 0..4 {
                skeleton::poster(ui, theme, Vec2::new(theme.tile_w, theme.tile_h), pulse);
            }
        });
    });
}

fn scroll_page(ui: &mut Ui, id: TmdbId, to_top: bool, add: impl FnOnce(&mut Ui)) {
    let salt = ("person-page", id);
    if to_top {
        scroll::vertical_to_top(ui, salt, add);
        return;
    }

    scroll::vertical(ui, salt, add);
}

struct Hero<'a> {
    profile_path: Option<&'a str>,
    name: &'a str,
    photo: Vec2,
    name_size: f32,
}

fn hero(ui: &mut Ui, svc: &Services, theme: &Theme, hero: Hero<'_>, extra: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        poster::rounded_image(ui, hero.photo, theme, || {
            let url = tmdb_image_url(hero.profile_path, "w185");
            svc.images.slot(url.as_deref())
        });

        ui.vertical(|ui| {
            ui.label(
                RichText::new(hero.name)
                    .font(theme.title_font(hero.name_size))
                    .color(theme.title),
            );
            extra(ui);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_person() -> CreditPerson {
        CreditPerson {
            id: TmdbId::new(7),
            name: "Timothée Chalamet".into(),
            role: "Paul Atreides".into(),
            profile_path: Some("/tim.jpg".into()),
        }
    }

    #[test]
    fn seed_starts_intro_once() {
        let mut screen = PersonScreen::default();
        screen.seed(sample_person());
        screen.start_intro_if_pending(1.0);
        assert!(intro::running(screen.intro_at, 1.05));

        screen.start_intro_if_pending(1.1);
        assert!((intro::t(screen.intro_at, 1.1) - intro::t(Some(1.0), 1.1)).abs() < f32::EPSILON);
    }
}
