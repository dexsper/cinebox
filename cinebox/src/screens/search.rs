//! Name-search results: tabbed poster grid with TMDB page loads on scroll.

use cinebox_core::i18n::Msg;
use cinebox_core::{CatalogItem, UiLanguage};
use cinebox_tmdb::{CatalogPage, SearchKind};
use egui::{RichText, Sense, Ui, vec2};
use egui_async::Bind;

use crate::jobs::{self, JobError};
use crate::nav::NavAction;
use crate::screens::paged::apply_page;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::{self, poster, scroll};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchTab {
    Movies = 0,
    Tv = 1,
    Actors = 2,
}

impl SearchTab {
    const ALL: [Self; 3] = [Self::Movies, Self::Tv, Self::Actors];

    fn index(self) -> usize {
        self as usize
    }

    fn kind(self) -> SearchKind {
        match self {
            Self::Movies => SearchKind::Movie,
            Self::Tv => SearchKind::Tv,
            Self::Actors => SearchKind::Person,
        }
    }

    fn label(self) -> Msg {
        match self {
            Self::Movies => Msg::SearchMovies,
            Self::Tv => Msg::SearchTv,
            Self::Actors => Msg::SearchActors,
        }
    }
}

struct TabState {
    items: Vec<CatalogItem>,
    next_page: u32,
    has_more: bool,
    loading: bool,
    page: Bind<CatalogPage, JobError>,
}

impl Default for TabState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            next_page: 1,
            has_more: true,
            loading: false,
            page: Bind::new(true),
        }
    }
}

impl TabState {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

pub struct SearchScreen {
    query: String,
    tab: SearchTab,
    tabs: [TabState; 3],
    lang: Option<UiLanguage>,
    reset_scroll: bool,
}

impl Default for SearchScreen {
    fn default() -> Self {
        Self {
            query: String::new(),
            tab: SearchTab::Movies,
            tabs: std::array::from_fn(|_| TabState::default()),
            lang: None,
            reset_scroll: false,
        }
    }
}

impl SearchScreen {
    pub fn seed(&mut self, query: String) {
        let query = query.trim().to_owned();
        if self.query == query {
            return;
        }

        self.query = query;
        self.tab = SearchTab::Movies;

        for tab in &mut self.tabs {
            tab.reset();
        }

        self.reset_scroll = true;
    }

    /// Drop live pages so the next paint reloads for a new TMDB language/key.
    pub fn forget_live(&mut self) {
        self.lang = None;

        for tab in &mut self.tabs {
            tab.reset();
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        if svc.settings.tmdb.api_key.is_empty() {
            return need_key(ui, theme);
        }

        if self.query.is_empty() {
            widgets::page_message(ui, theme, Msg::EmptyRow.t(), theme.muted);
            return None;
        }

        let lang = svc.settings.general.language;
        if self.lang != Some(lang) {
            let switched = self.lang.is_some();
            self.lang = Some(lang);
            if switched {
                for tab in &mut self.tabs {
                    tab.reset();
                }
                self.reset_scroll = true;
            }
        }

        self.take_page();
        let failed = matches!(self.current_mut().page.read(), Some(Err(_)));
        let should_load = {
            let tab = self.current();
            tab.items.is_empty() && tab.has_more && !tab.loading && !failed
        };

        if should_load {
            self.start_load(svc);
        }

        self.paint_tabs(ui, theme);
        let tab = self.current();

        if tab.items.is_empty() {
            return self.empty_view(ui, theme, failed);
        }

        self.grid(ui, svc, theme, failed)
    }

    fn paint_tabs(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for tab in SearchTab::ALL {
                let active = self.tab == tab;
                if !button::label(ui, theme, tab.label().t(), Opts::chip(active)) {
                    continue;
                }

                if active {
                    continue;
                }

                self.tab = tab;
            }
        });
        ui.add_space(8.0);
    }

    fn grid(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        failed: bool,
    ) -> Option<NavAction> {
        let to_top = self.reset_scroll;
        self.reset_scroll = false;

        let mut near_end = false;
        let mut action = None;
        let tab = self.tab;
        let loading = self.current().loading;

        scroll_page(ui, tab, to_top, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                for item in &self.current().items {
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

            if loading {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(28.0).color(theme.muted));
                });
            }

            if failed {
                ui.add_space(12.0);
                let error = match self.current_mut().page.read() {
                    Some(Err(error)) => error.to_string(),
                    _ => Msg::Failed.t().to_owned(),
                };

                ui.label(RichText::new(error).size(theme.text_small).color(theme.err));
                ui.add_space(8.0);

                if widgets::button::label(
                    ui,
                    theme,
                    Msg::Retry.t(),
                    widgets::button::Opts::secondary(vec2(128.0, crate::widgets::combo::HEIGHT)),
                ) {
                    self.current_mut().page.clear();
                    self.current_mut().loading = false;
                }
            }
        });

        let tab = self.current();
        let load_more = near_end && tab.has_more && !tab.loading && !failed;
        if load_more {
            self.start_load(svc);
        }

        if self.current().loading {
            ui.ctx().request_repaint();
        }

        action
    }

    fn empty_view(&mut self, ui: &mut Ui, theme: &Theme, failed: bool) -> Option<NavAction> {
        if failed {
            let error = match self.current_mut().page.read() {
                Some(Err(error)) => error.to_string(),
                _ => Msg::Failed.t().to_owned(),
            };

            if widgets::page_error(ui, theme, &error) {
                self.current_mut().page.clear();
                self.current_mut().loading = false;
            }
            return None;
        }

        let waiting = self.current().loading || self.current().has_more;
        if waiting {
            if self.current().loading {
                ui.ctx().request_repaint();
            }
            widgets::page_spinner(ui, theme);
            return None;
        }

        widgets::page_message(ui, theme, Msg::EmptyRow.t(), theme.muted);
        None
    }

    fn take_page(&mut self) {
        let tab = self.current_mut();
        let page = match tab.page.read() {
            Some(Ok(page)) => page.clone(),
            Some(Err(_)) => {
                tab.loading = false;
                return;
            }
            None => return,
        };

        tab.page.clear();
        tab.loading = false;
        apply_page(&mut tab.items, &mut tab.next_page, &mut tab.has_more, page);
    }

    fn start_load(&mut self, svc: &Services) {
        if self.current().loading {
            return;
        }

        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        let query = self.query.clone();
        let kind = self.tab.kind();
        let page = self.current().next_page;
        let tab = self.current_mut();
        tab.loading = true;

        let _ = tab
            .page
            .read_or_request(move || jobs::load_search_page(tmdb, query, kind, page));
    }

    fn current(&self) -> &TabState {
        &self.tabs[self.tab.index()]
    }

    fn current_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.tab.index()]
    }
}

fn need_key(ui: &mut Ui, theme: &Theme) -> Option<NavAction> {
    ui.label(RichText::new(Msg::NeedTmdbKey.t()).color(theme.muted));
    let settings_size = vec2(160.0, crate::widgets::combo::HEIGHT);

    if crate::widgets::button::label(
        ui,
        theme,
        Msg::NavSettings.t(),
        crate::widgets::button::Opts::secondary(settings_size),
    ) {
        return Some(NavAction::OpenSettings);
    }

    None
}

fn scroll_page(ui: &mut Ui, tab: SearchTab, to_top: bool, add: impl FnOnce(&mut Ui)) {
    let salt = ("search-page", tab as u8);
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
    fn seed_resets_tabs_and_new_query() {
        let mut screen = SearchScreen::default();
        screen.seed(String::from("  dune  "));
        screen.tabs[0].items.push(movie(1, "A"));
        screen.tab = SearchTab::Tv;

        screen.seed(String::from("alien"));

        assert_eq!(screen.query, "alien");
        assert_eq!(screen.tab, SearchTab::Movies);
        assert!(screen.tabs[0].items.is_empty());
        assert!(screen.reset_scroll);
    }

    #[test]
    fn same_query_does_not_reset() {
        let mut screen = SearchScreen::default();
        screen.seed(String::from("dune"));
        screen.tabs[0].items.push(movie(1, "A"));

        screen.seed(String::from("dune"));

        assert_eq!(screen.tabs[0].items.len(), 1);
    }

    #[test]
    fn tabs_keep_separate_items() {
        let mut screen = SearchScreen::default();
        screen.seed(String::from("dune"));
        screen.tabs[SearchTab::Movies.index()]
            .items
            .push(movie(1, "A"));

        screen.tabs[SearchTab::Tv.index()].items.push(movie(2, "B"));
        assert_eq!(screen.current().items[0].title, "A");

        screen.tab = SearchTab::Tv;
        assert_eq!(screen.current().items[0].title, "B");

        screen.tab = SearchTab::Movies;
        assert_eq!(screen.current().items[0].title, "A");
    }
}
