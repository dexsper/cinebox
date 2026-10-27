//! Name-search results: tabbed poster grid with TMDB page loads on scroll.

use cinebox_core::UiLanguage;
use cinebox_tmdb::SearchKind;
use egui::Ui;
use rust_i18n::t;

use super::gate;
use super::paged::{self, PagedGrid};
use crate::app::nav::NavAction;
use crate::i18n::errors::UserError;
use crate::services::{Services, jobs};
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::page_state::{ErrorChoice, error_page};
use crate::widgets::{self, scroll};

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

    fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::Movies => t!("search.movies"),
            Self::Tv => t!("search.tv"),
            Self::Actors => t!("search.actors"),
        }
    }
}

pub(crate) struct SearchScreen {
    query: String,
    tab: SearchTab,
    tabs: [PagedGrid; 3],
    lang: Option<UiLanguage>,
    is_scroll_reset: bool,
}

impl Default for SearchScreen {
    fn default() -> Self {
        Self {
            query: String::new(),
            tab: SearchTab::Movies,
            tabs: std::array::from_fn(|_| PagedGrid::default()),
            lang: None,
            is_scroll_reset: false,
        }
    }
}

impl SearchScreen {
    pub(crate) fn seed(&mut self, query: String) {
        let query = query.trim().to_owned();
        if self.query == query {
            return;
        }

        self.query = query;
        self.tab = SearchTab::Movies;
        self.reset_tabs();
        self.is_scroll_reset = true;
    }

    pub(crate) fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        if let Some(problem) = gate::tmdb_key_problem(&svc.settings) {
            return gate::tmdb_key(ui, theme, problem);
        }

        if self.query.is_empty() {
            widgets::page_message(ui, theme, t!("catalog.empty").as_ref(), theme.muted);
            return None;
        }

        self.follow_language(svc.settings.general.language);

        let grid = self.current_mut();
        grid.take_page();

        if grid.items.is_empty() && grid.error().is_none() {
            self.start_load(svc);
        }

        self.paint_tabs(ui, theme);

        if self.current().items.is_empty() {
            return self.empty_view(ui, theme);
        }

        self.grid(ui, svc, theme)
    }

    /// The first language seen is only recorded; a later switch reloads every tab.
    fn follow_language(&mut self, lang: UiLanguage) {
        let previous = self.lang.replace(lang);
        if previous.is_none() || previous == Some(lang) {
            return;
        }

        self.reset_tabs();
        self.is_scroll_reset = true;
    }

    fn paint_tabs(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            for tab in SearchTab::ALL {
                if button::label(ui, theme, tab.label().as_ref(), Opts::chip(self.tab == tab)) {
                    self.tab = tab;
                }
            }
        });
        ui.add_space(8.0);
    }

    fn grid(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme) -> Option<NavAction> {
        let tab = self.tab;
        let is_scroll_reset = std::mem::take(&mut self.is_scroll_reset);
        let grid = self.current_mut();

        let is_loading = grid.is_loading();
        let error = grid.error().map(|error| UserError::from(error).summary());

        let mut is_near_end = false;
        let mut is_retry_clicked = false;
        let mut action = None;

        scroll::page(ui, ("search-page", tab as u8), is_scroll_reset, |ui| {
            action = paged::tile_grid(ui, svc, theme, &grid.items);
            is_near_end = paged::is_end_in_view(ui);

            if is_loading {
                paged::loading_row(ui, theme);
            }

            if let Some(error) = &error {
                is_retry_clicked = paged::retry_row(ui, theme, error);
            }
        });

        if is_retry_clicked {
            grid.retry();
        }

        if is_near_end && error.is_none() {
            self.start_load(svc);
        }

        if self.current().is_loading() {
            ui.ctx().request_repaint();
        }

        action
    }

    fn empty_view(&mut self, ui: &mut Ui, theme: &Theme) -> Option<NavAction> {
        let grid = self.current_mut();

        if let Some(error) = grid.error().map(UserError::from) {
            let choice = error_page(ui, theme, &error);
            if choice == Some(ErrorChoice::Retry) {
                grid.retry();
            }

            return choice.and_then(ErrorChoice::nav);
        }

        if grid.is_loading() {
            ui.ctx().request_repaint();
        }

        if grid.is_loading() || grid.has_more() {
            widgets::page_spinner(ui, theme);
            return None;
        }

        widgets::page_message(ui, theme, t!("catalog.empty").as_ref(), theme.muted);

        None
    }

    fn start_load(&mut self, svc: &Services) {
        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        let query = self.query.clone();
        let kind = self.tab.kind();

        self.current_mut().request(move |page| jobs::load_search_page(tmdb, query, kind, page));
    }

    fn reset_tabs(&mut self) {
        for tab in &mut self.tabs {
            tab.reset();
        }
    }

    fn current(&self) -> &PagedGrid {
        &self.tabs[self.tab.index()]
    }

    fn current_mut(&mut self) -> &mut PagedGrid {
        &mut self.tabs[self.tab.index()]
    }
}

impl super::LiveTmdb for SearchScreen {
    fn forget_live(&mut self) {
        self.lang = None;
        self.reset_tabs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cinebox_core::{CatalogItem, MediaKind, TmdbId};

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
        assert_eq!(screen.tabs[0].items, [] as [CatalogItem; 0]);
        assert!(screen.is_scroll_reset);
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
        screen.tabs[SearchTab::Movies.index()].items.push(movie(1, "A"));

        screen.tabs[SearchTab::Tv.index()].items.push(movie(2, "B"));
        assert_eq!(screen.current().items[0].title, "A");

        screen.tab = SearchTab::Tv;
        assert_eq!(screen.current().items[0].title, "B");

        screen.tab = SearchTab::Movies;
        assert_eq!(screen.current().items[0].title, "A");
    }

    #[test]
    fn language_switch_resets_tabs() {
        let mut screen = SearchScreen::default();
        screen.follow_language(UiLanguage::English);
        screen.tabs[0].items.push(movie(1, "A"));

        screen.follow_language(UiLanguage::English);
        assert_eq!(screen.tabs[0].items.len(), 1);
        assert!(!screen.is_scroll_reset);

        screen.follow_language(UiLanguage::Russian);
        assert_eq!(screen.tabs[0].items, [] as [CatalogItem; 0]);
        assert!(screen.is_scroll_reset);
    }
}
