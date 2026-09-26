//! Section hub: themed shelves for movies, cartoons, TV, or anime, plus genre shortcuts.

use std::collections::HashMap;

use cinebox_core::{Section, language_key};
use cinebox_tmdb::{ShelfRow, genres_for};
use egui::{RichText, Ui, vec2};
use egui_material_icons::icons::ICON_TUNE;
use rust_i18n::t;

use super::discover::DiscoverFilters;
use super::shelf::shelf;
use super::swr::{Cached, Swr};
use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::{self, scroll};

type SectionCache = Cached<Vec<ShelfRow>, (Vec<ShelfRow>, bool)>;

#[derive(Default)]
pub struct SectionScreen {
    caches: HashMap<Section, SectionCache>,
}

impl SectionScreen {
    /// Drop every hub so the next paint reloads for a new TMDB key, language, or network.
    pub fn forget_live(&mut self) {
        self.caches.clear();
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        section: Section,
    ) -> Option<NavAction> {
        if svc.settings.tmdb.api_key.is_empty() {
            ui.label(RichText::new(t!("catalog.need_tmdb_key").as_ref()).color(theme.muted));
            return None;
        }

        let cache = self.caches.entry(section).or_default();
        cache.sync_lang(svc.settings.general.language);

        let lang = language_key(Some(svc.settings.general.language.tmdb_code())).to_owned();
        let db = svc.db.clone();
        let hydrated = cache.hydrate(async move { jobs::cached_section(db?, lang, section).await });
        if !hydrated {
            widgets::page_spinner(ui, theme);
            return None;
        }

        let fresh = cache.disk.as_ref().is_some_and(|(_, fresh)| *fresh);
        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        let db = svc.db.clone();
        let outcome = cache.resolve(fresh, move || jobs::load_section(tmdb, section, db));

        let rows = match outcome.view {
            Swr::Live => match cache.bind.read() {
                Some(Ok(rows)) => Some(rows.as_slice()),
                _ => None,
            },
            Swr::Disk => cache.disk.as_ref().map(|(rows, _)| rows.as_slice()),
            Swr::Failed => {
                let error = match cache.bind.read() {
                    Some(Err(error)) => error.to_string(),
                    _ => t!("common.failed").into_owned(),
                };
                if widgets::page_error(ui, theme, &error) {
                    cache.retry();
                }
                return None;
            }
            Swr::Pending => {
                widgets::page_spinner(ui, theme);
                return None;
            }
        };

        let mut action = None;
        scroll::vertical(ui, ("section-page", section.as_key()), |ui| {
            action = header(ui, section, theme);

            for row in rows.unwrap_or_default() {
                if !row.id.is_remote() && row.items.is_empty() {
                    continue;
                }

                let nav = shelf(ui, row.id, &row.items, row.error.as_deref(), svc, theme);
                if nav.is_some() {
                    action = nav;
                }
            }
        });

        action
    }
}

fn header(ui: &mut Ui, section: Section, theme: &Theme) -> Option<NavAction> {
    let mut action = None;
    let kind = section.primary_kind();

    ui.add_space(8.0);
    ui.label(
        RichText::new(crate::i18n::section_title(section).as_ref())
            .font(theme.title_font(theme.text_heading))
            .color(theme.title),
    );
    ui.add_space(10.0);

    scroll::horizontal(ui, ("section-genres", section.as_key()), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            let discover = t!("discover.title");
            let pad_y = button::icon_label_pad_y(ui, theme, ICON_TUNE, button::CHIP_H);
            let opts = Opts::primary(vec2(button::CHIP_MIN_W, button::CHIP_H)).pad_y(pad_y);
            if button::icon_label(ui, theme, ICON_TUNE, discover.as_ref(), opts) {
                action = Some(NavAction::OpenDiscover {
                    section,
                    filters: DiscoverFilters::new(kind),
                });
            }

            for genre in genres_for(kind) {
                let name = crate::i18n::genre_name(*genre);
                if button::label(ui, theme, name.as_ref(), Opts::chip(false)) {
                    action = Some(NavAction::OpenDiscover {
                        section,
                        filters: DiscoverFilters::with_genre(kind, *genre),
                    });
                }
            }
        });
    });

    action
}
