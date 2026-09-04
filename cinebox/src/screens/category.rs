//! Full-shelf grid: items already on Home, then extra TMDB pages on scroll.

use cinebox_core::{CatalogItem, HomeRowId, UiLanguage};
use cinebox_tmdb::CatalogPage;
use egui::{RichText, Sense, Ui, vec2};
use egui_async::Bind;
use rust_i18n::t;

use crate::jobs::{self, JobError};
use crate::nav::NavAction;
use crate::screens::paged::apply_page;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, poster, scroll};

pub struct CategoryScreen {
    id: Option<HomeRowId>,
    items: Vec<CatalogItem>,
    next_page: u32,
    has_more: bool,
    loading: bool,
    page: Bind<CatalogPage, JobError>,
    lang: Option<UiLanguage>,
    reset_scroll: bool,
}

impl Default for CategoryScreen {
    fn default() -> Self {
        Self {
            id: None,
            items: Vec::new(),
            next_page: 1,
            has_more: false,
            loading: false,
            page: Bind::new(true),
            lang: None,
            reset_scroll: false,
        }
    }
}

impl CategoryScreen {
    pub fn seed(&mut self, id: HomeRowId, items: Vec<CatalogItem>) {
        self.id = Some(id);
        self.has_more = id.is_remote();
        self.loading = false;
        self.page = Bind::new(true);
        self.reset_scroll = true;

        if items.is_empty() {
            self.next_page = 1;
            self.items = items;
            return;
        }

        self.next_page = 2;
        self.items = items;
    }

    /// Drop live pages so the next paint reloads for a new TMDB language/key.
    pub fn forget_live(&mut self) {
        self.lang = None;
        self.page = Bind::new(true);
        self.loading = false;

        let Some(id) = self.id else {
            return;
        };

        if !id.is_remote() {
            return;
        }

        self.items.clear();
        self.next_page = 1;
        self.has_more = true;
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        id: HomeRowId,
    ) -> Option<NavAction> {
        if self.id != Some(id) {
            self.seed(id, Vec::new());
        }

        let lang = svc.settings.general.language;
        if self.lang != Some(lang) {
            let switched = self.lang.is_some();
            self.lang = Some(lang);
            if switched {
                self.page = Bind::new(true);
                self.loading = false;
                if id.is_remote() {
                    self.items.clear();
                    self.next_page = 1;
                    self.has_more = true;
                }
            }
        }

        self.take_page();

        let failed = matches!(self.page.read(), Some(Err(_)));
        if self.items.is_empty() && self.has_more && !self.loading && !failed {
            self.start_load(svc);
        }

        if self.items.is_empty() {
            return self.empty_view(ui, theme, failed);
        }

        let to_top = self.reset_scroll;
        self.reset_scroll = false;

        let mut near_end = false;
        let mut action = None;

        scroll_page(ui, id, to_top, |ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new(crate::i18n::home_row_title(id).as_ref())
                    .font(theme.title_font(theme.text_heading))
                    .color(theme.title),
            );

            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                for item in &self.items {
                    let opened = poster::catalog_tile(
                        ui,
                        item,
                        &svc.images,
                        svc.settings.tmdb.poster_size,
                        theme,
                        svc.is_watched(item.kind, item.id),
                    );
                    if action.is_none() {
                        action = opened;
                    }
                }
            });

            let sentinel_size = vec2(ui.available_width(), 1.0);
            let (sentinel, _) = ui.allocate_exact_size(sentinel_size, Sense::hover());
            near_end = poster::in_load_window(ui, sentinel);

            if self.loading {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(28.0).color(theme.muted));
                });
            }

            if failed {
                ui.add_space(12.0);
                let error = match self.page.read() {
                    Some(Err(error)) => error.to_string(),
                    _ => t!("common.failed").into_owned(),
                };

                ui.label(RichText::new(error).size(theme.text_small).color(theme.err));
                ui.add_space(8.0);

                if widgets::button::label(
                    ui,
                    theme,
                    t!("common.retry").as_ref(),
                    widgets::button::Opts::secondary(vec2(128.0, crate::widgets::combo::HEIGHT)),
                ) {
                    self.page.clear();
                    self.loading = false;
                }
            }
        });

        if near_end && self.has_more && !self.loading && !failed {
            self.start_load(svc);
        }

        if self.loading {
            ui.ctx().request_repaint();
        }

        action
    }

    fn empty_view(&mut self, ui: &mut Ui, theme: &Theme, failed: bool) -> Option<NavAction> {
        if failed {
            let error = match self.page.read() {
                Some(Err(error)) => error.to_string(),
                _ => t!("common.failed").into_owned(),
            };
            if widgets::page_error(ui, theme, &error) {
                self.page.clear();
                self.loading = false;
            }
            return None;
        }

        if self.loading || self.has_more {
            if self.loading {
                ui.ctx().request_repaint();
            }
            widgets::page_spinner(ui, theme);
            return None;
        }

        widgets::page_message(ui, theme, t!("catalog.empty").as_ref(), theme.muted);
        None
    }

    fn take_page(&mut self) {
        let page = match self.page.read() {
            Some(Ok(page)) => page.clone(),
            Some(Err(_)) => {
                self.loading = false;
                return;
            }
            None => return,
        };

        self.page.clear();
        self.loading = false;
        apply_page(
            &mut self.items,
            &mut self.next_page,
            &mut self.has_more,
            page,
        );
    }

    fn start_load(&mut self, svc: &Services) {
        if self.loading {
            return;
        }

        let Some(id) = self.id else {
            return;
        };

        if !id.is_remote() {
            self.has_more = false;
            return;
        }

        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        let page = self.next_page;
        self.loading = true;
        let _ = self
            .page
            .read_or_request(move || jobs::load_catalog_page(tmdb, id, page));
    }
}

fn scroll_page(ui: &mut Ui, id: HomeRowId, to_top: bool, add: impl FnOnce(&mut Ui)) {
    let salt = ("category-page", id.as_key());
    if to_top {
        scroll::vertical_to_top(ui, salt, add);
        return;
    }

    scroll::vertical(ui, salt, add);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cinebox_core::{MediaKind, TmdbId};

    fn movie(id: u32, title: &str) -> CatalogItem {
        CatalogItem {
            id: TmdbId::new(id),
            kind: MediaKind::Movie,
            title: title.to_owned(),
            year: Some(2024),
            vote: None,
            poster_path: None,
        }
    }

    #[test]
    fn seed_keeps_home_items_and_starts_at_page_two() {
        let mut screen = CategoryScreen::default();
        screen.seed(HomeRowId::NowPlaying, vec![movie(1, "A")]);

        assert_eq!(screen.items.len(), 1);
        assert_eq!(screen.next_page, 2);
        assert!(screen.has_more);
    }

    #[test]
    fn recently_watched_does_not_paginate() {
        let mut screen = CategoryScreen::default();
        screen.seed(HomeRowId::RecentlyWatched, vec![movie(1, "A")]);

        assert!(!screen.has_more);
    }

    #[test]
    fn empty_seed_starts_at_first_page() {
        let mut screen = CategoryScreen::default();
        screen.seed(HomeRowId::PopularMovies, Vec::new());

        assert!(screen.items.is_empty());
        assert_eq!(screen.next_page, 1);
        assert!(screen.has_more);
    }
}
