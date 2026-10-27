use cinebox_core::{
    CacheHit, CacheKey, CreditPerson, DETAILS_TTL, KIND_PERSON, PersonDetails, TmdbId, language_key, person_cache_id,
    tmdb_image_url,
};
use egui::{RichText, Ui, Vec2};
use rust_i18n::t;

use super::gate;
use crate::app::nav::NavAction;
use crate::i18n::errors::UserError;
use crate::services::Services;
use crate::services::jobs;
use crate::theme::Theme;
use crate::widgets::page_state::{ErrorChoice, error_page};
use crate::widgets::poster::TileLook;
use crate::widgets::{self, intro, poster, scroll, skeleton};

const FROM_W: f32 = 100.0;
const FROM_H: f32 = 150.0;
const TO_W: f32 = 140.0;
const TO_H: f32 = 210.0;

/// Where the page stands this frame: how far the hero has grown, and whether
/// it jumps back to the top.
#[derive(Clone, Copy)]
struct PageLayout {
    t: f32,
    is_scroll_reset: bool,
}

#[derive(Default)]
pub(crate) struct PersonScreen {
    id: Option<TmdbId>,
    preview: Option<CreditPerson>,
    cache: super::swr::Cached<Box<PersonDetails>, CacheHit<Box<PersonDetails>>>,
    intro_at: Option<f64>,
    pending_intro: bool,
    is_scroll_reset: bool,
}

struct Hero<'a> {
    profile_path: Option<&'a str>,
    name: &'a str,
    photo: Vec2,
    name_size: f32,
}

impl<'a> Hero<'a> {
    /// The photo and name at `t` of the intro.
    fn at(theme: &Theme, t: f32, profile_path: Option<&'a str>, name: &'a str) -> Self {
        let photo = Vec2::new(intro::lerp(FROM_W, TO_W, t), intro::lerp(FROM_H, TO_H, t));
        let name_size = intro::lerp(theme.text_caption, theme.text_person, t);

        Self { profile_path, name, photo, name_size }
    }
}

impl PersonScreen {
    pub(crate) fn seed(&mut self, person: CreditPerson) {
        if self.id != Some(person.id) {
            self.cache.reset();
        }

        self.id = Some(person.id);
        self.preview = Some(person);
        self.pending_intro = true;
        self.is_scroll_reset = true;
    }

    pub(crate) fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme, id: TmdbId) -> Option<NavAction> {
        if let Some(problem) = gate::tmdb_key_problem(&svc.settings) {
            return gate::tmdb_key(ui, theme, problem);
        }

        let now = ui.input(|i| i.time);
        if self.id != Some(id) {
            self.id = Some(id);
            self.cache.reset();
            self.is_scroll_reset = true;
            if self.preview.as_ref().is_none_or(|person| person.id != id) {
                self.preview = None;
            }
        }

        self.cache.sync_lang(svc.settings.general.language);

        let lang = language_key(Some(svc.settings.general.language.tmdb_code())).to_owned();
        let disk_db = svc.db.clone();
        let hydrated = self.cache.hydrate(async move {
            let db = disk_db?;
            let cache_id = person_cache_id(id);
            let hit = db
                .get_json::<PersonDetails>(CacheKey { language: &lang, kind: KIND_PERSON, id: &cache_id })
                .await
                .ok()
                .flatten()?;

            let mut value = hit.value;
            value.apply_typography();

            Some(CacheHit { value: Box::new(value), fetched_at: hit.fetched_at })
        });
        self.start_intro_if_pending(now);

        if intro::running(self.intro_at, now) {
            ui.ctx().request_repaint();
        }

        let layout = PageLayout { t: intro::t(self.intro_at, now), is_scroll_reset: self.is_scroll_reset };
        if !hydrated {
            if let Some(person) = self.preview.as_ref() {
                self.is_scroll_reset = false;
                loading(ui, svc, theme, layout, person);
            } else {
                widgets::page_spinner(ui, theme);
            }
            ui.ctx().request_repaint();
            return None;
        }

        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        let db = svc.db.clone();
        let fresh = self.cache.disk.as_ref().is_some_and(|hit| hit.is_fresh(DETAILS_TTL));

        let outcome = self.cache.resolve(fresh, move || jobs::load_person(tmdb, id, db));
        if outcome.in_flight {
            ui.ctx().request_repaint();
        }

        let mut retry = false;
        let action = match outcome.view {
            super::swr::Swr::Live => match self.cache.bind.read() {
                Some(Ok(details)) => {
                    self.is_scroll_reset = false;
                    ready(ui, details, svc, theme, layout)
                }
                _ => None,
            },
            super::swr::Swr::Disk => match self.cache.disk.as_ref() {
                Some(hit) => {
                    self.is_scroll_reset = false;
                    ready(ui, &hit.value, svc, theme, layout)
                }
                None => None,
            },
            super::swr::Swr::Failed => {
                let error = UserError::from_read(self.cache.bind.read().as_ref());
                let choice = error_page(ui, theme, &error);
                retry = choice == Some(ErrorChoice::Retry);
                choice.and_then(ErrorChoice::nav)
            }
            super::swr::Swr::Pending => {
                if let Some(person) = self.preview.as_ref() {
                    self.is_scroll_reset = false;
                    loading(ui, svc, theme, layout, person);
                } else {
                    widgets::page_spinner(ui, theme);
                }

                None
            }
        };

        if retry {
            self.cache.retry();
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

impl super::LiveTmdb for PersonScreen {
    fn forget_live(&mut self) {
        self.cache.forget_live();
    }
}

fn ready(ui: &mut Ui, details: &PersonDetails, svc: &Services, theme: &Theme, layout: PageLayout) -> Option<NavAction> {
    let mut action = None;
    let card = Hero::at(theme, layout.t, details.profile_path.as_deref(), &details.name);

    scroll_page(ui, details.id, layout.is_scroll_reset, |ui| {
        hero(ui, svc, theme, &card, |ui| {
            if let Some(born) = details.birthday.as_deref() {
                ui.label(RichText::new(born).size(theme.text_small).color(theme.muted));
            }

            if let Some(place) = details.place_of_birth.as_deref() {
                ui.label(RichText::new(place).size(theme.text_small).color(theme.muted));
            }
        });

        if let Some(bio) = details.biography.as_deref() {
            ui.add_space(12.0);
            ui.label(RichText::new(bio).size(theme.text_body).color(theme.body));
        }

        if !details.credits.is_empty() {
            ui.add_space(12.0);
            ui.label(
                RichText::new(t!("person.credits").as_ref())
                    .font(theme.title_font(theme.text_section))
                    .color(theme.title),
            );

            let scale = poster::card_scale(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                for item in &details.credits {
                    let look = TileLook { marks: svc.tile_marks(item.kind, item.id), scale };
                    if let Some(nav) = poster::catalog_tile(ui, theme, item, svc.posters(), look) {
                        action = Some(nav);
                    }
                }
            });
        }
    });

    action
}

fn loading(ui: &mut Ui, svc: &Services, theme: &Theme, layout: PageLayout, person: &CreditPerson) {
    let card = Hero::at(theme, layout.t, person.profile_path.as_deref(), &person.name);
    let pulse = skeleton::pulse(ui);

    scroll_page(ui, person.id, layout.is_scroll_reset, |ui| {
        hero(ui, svc, theme, &card, |_| {});
        ui.add_space(8.0);
        skeleton::bar(ui, theme, 160.0, 13.0, pulse);
        ui.add_space(6.0);
        skeleton::bar(ui, theme, 220.0, 13.0, pulse);
        ui.add_space(16.0);
        skeleton::bar(ui, theme, ui.available_width().min(theme.overview_max_w), 14.0, pulse);
        ui.add_space(6.0);
        skeleton::bar(ui, theme, ui.available_width().min(theme.overview_max_w) * 0.9, 14.0, pulse);
        ui.add_space(6.0);
        skeleton::bar(ui, theme, ui.available_width().min(theme.overview_max_w) * 0.65, 14.0, pulse);

        ui.add_space(16.0);
        skeleton::bar(ui, theme, 100.0, 16.0, pulse);
        ui.add_space(8.0);

        let scale = poster::card_scale(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for _ in 0..4 {
                skeleton::poster(ui, theme, Vec2::new(theme.tile_w, theme.tile_h) * scale, pulse);
            }
        });
    });
}

fn scroll_page(ui: &mut Ui, id: TmdbId, is_scroll_reset: bool, add: impl FnOnce(&mut Ui)) {
    scroll::page(ui, ("person-page", id), is_scroll_reset, add);
}

fn hero(ui: &mut Ui, svc: &Services, theme: &Theme, hero: &Hero<'_>, extra: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        poster::rounded_image(ui, hero.photo, theme, || {
            let url = tmdb_image_url(hero.profile_path, "w185");
            svc.images.slot(url.as_deref())
        });

        ui.vertical(|ui| {
            ui.label(RichText::new(hero.name).font(theme.title_font(hero.name_size)).color(theme.title));
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
