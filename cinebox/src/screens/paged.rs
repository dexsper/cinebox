//! Shared poster-grid pagination used by category, discover, and search screens.

use std::collections::HashSet;
use std::future::Future;

use cinebox_core::CatalogItem;
use cinebox_tmdb::CatalogPage;
use egui::{AsIdSalt, RichText, Sense, Ui, vec2};
use egui_async::Bind;
use rust_i18n::t;

use crate::app::nav::NavAction;
use crate::i18n::errors::UserError;
use crate::services::Services;
use crate::services::jobs::JobError;
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::poster::{self, TileLook};
use crate::widgets::{self, scroll};

/// Infinite poster grid over one TMDB list.
pub(super) struct PagedGrid {
    pub items: Vec<CatalogItem>,
    next_page: u32,
    has_more: bool,
    is_loading: bool,
    page: Bind<CatalogPage, JobError>,
}

impl Default for PagedGrid {
    fn default() -> Self {
        Self { items: Vec::new(), next_page: 1, has_more: true, is_loading: false, page: Bind::new(true) }
    }
}

/// Where the items of a [`PagedGrid::seeded`] grid come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    /// A TMDB list: the grid pages on after the seed.
    Remote,

    /// A local list: the seed is all there is.
    Local,
}

/// How [`PagedGrid::ui`] lays out its scroll.
pub(super) struct GridView<S, H> {
    pub scroll_salt: S,
    /// This frame starts at the top of the scroll.
    pub is_scroll_reset: bool,
    /// Drawn above the grid, inside the scroll.
    pub header: H,
}

pub(super) struct GridOut {
    pub action: Option<NavAction>,
    /// Caller should [`PagedGrid::request`] the next page.
    pub is_page_wanted: bool,
}

impl PagedGrid {
    /// Start from items already shown elsewhere (a Home shelf).
    pub(super) fn seeded(items: Vec<CatalogItem>, source: Source) -> Self {
        let next_page = if items.is_empty() { 1 } else { 2 };
        let has_more = source == Source::Remote;

        Self { items, next_page, has_more, ..Self::default() }
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub(super) fn has_more(&self) -> bool {
        self.has_more
    }

    /// The error of the last page load, kept until [`Self::retry`].
    pub(super) fn error(&mut self) -> Option<&JobError> {
        self.page.err_ref()
    }

    pub(super) fn retry(&mut self) {
        self.page.clear();
        self.is_loading = false;
    }

    /// Start loading the next page unless one is already in flight or the list has ended.
    pub(super) fn request<Fut>(&mut self, load: impl FnOnce(u32) -> Fut)
    where
        Fut: Future<Output = Result<CatalogPage, JobError>> + Send + 'static,
    {
        if self.is_loading || !self.has_more {
            return;
        }

        self.is_loading = true;
        let page = self.next_page;
        let _ = self.page.read_or_request(|| load(page));
    }

    /// Append a page that finished loading. A failed load stays in place until [`Self::retry`].
    pub(super) fn take_page(&mut self) {
        if self.page.is_err() {
            self.is_loading = false;
            return;
        }

        let Some(page) = self.page.take_ok() else { return };

        self.is_loading = false;
        self.apply_page(page);
    }

    /// Header, then the grid (or its loading, error, or empty state), all in one scroll.
    pub(super) fn ui<S, H>(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme, view: GridView<S, H>) -> GridOut
    where
        S: AsIdSalt,
        H: FnOnce(&mut Ui),
    {
        self.take_page();

        let error = self.error().map(|error| UserError::from(error).summary());
        let mut is_near_end = false;
        let mut is_retry_clicked = false;
        let mut action = None;

        scroll::page(ui, view.scroll_salt, view.is_scroll_reset, |ui| {
            (view.header)(ui);
            action = tile_grid(ui, svc, theme, &self.items);
            is_near_end = is_end_in_view(ui);

            if let Some(error) = &error {
                is_retry_clicked = retry_row(ui, theme, error);
                return;
            }

            if self.is_loading || (self.items.is_empty() && self.has_more) {
                loading_row(ui, theme);
                return;
            }

            if self.items.is_empty() {
                ui.add_space(24.0);
                ui.label(
                    RichText::new(t!("catalog.empty").as_ref())
                        .font(theme.title_font(theme.text_display))
                        .color(theme.muted),
                );
            }
        });

        if is_retry_clicked {
            self.retry();
        }

        if self.is_loading {
            ui.ctx().request_repaint();
        }

        let wants_more = self.items.is_empty() || is_near_end;
        let can_load = self.has_more && !self.is_loading && error.is_none() && !is_retry_clicked;

        GridOut { action, is_page_wanted: wants_more && can_load }
    }

    fn apply_page(&mut self, page: CatalogPage) {
        extend_unique(&mut self.items, page.items);

        let seen = page.page.max(self.next_page);
        self.next_page = seen.saturating_add(1);
        self.has_more = page.page < page.total_pages;
    }
}

fn extend_unique(items: &mut Vec<CatalogItem>, incoming: Vec<CatalogItem>) {
    let mut seen: HashSet<_> = items.iter().map(|item| (item.id, item.kind)).collect();

    for item in incoming {
        let is_new = seen.insert((item.id, item.kind));
        if !is_new {
            continue;
        }

        items.push(item);
    }
}

/// Wrapped grid of catalog tiles; `Some` for the tile clicked this frame.
pub(super) fn tile_grid<'a>(
    ui: &mut Ui,
    svc: &Services,
    theme: &Theme,
    items: impl IntoIterator<Item = &'a CatalogItem>,
) -> Option<NavAction> {
    let scale = poster::card_scale(ui.available_width());
    let posters = svc.posters();
    let mut action = None;

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(12.0, 12.0);

        for item in items {
            let look = TileLook { marks: svc.tile_marks(item.kind, item.id), scale };
            if let Some(nav) = poster::catalog_tile(ui, theme, item, posters, look) {
                action = Some(nav);
            }
        }
    });

    action
}

/// Whether the bottom of the grid is inside the load window, so the next page should start.
pub(super) fn is_end_in_view(ui: &mut Ui) -> bool {
    let size = vec2(ui.available_width(), 1.0);
    let (sentinel, _) = ui.allocate_exact_size(size, Sense::hover());

    poster::in_load_window(ui, sentinel)
}

pub(super) fn loading_row(ui: &mut Ui, theme: &Theme) {
    ui.add_space(16.0);
    ui.vertical_centered(|ui| {
        ui.add(egui::Spinner::new().size(28.0).color(theme.muted));
    });
}

/// Inline load error under a grid; `true` when retry was clicked.
pub(super) fn retry_row(ui: &mut Ui, theme: &Theme, message: &str) -> bool {
    ui.add_space(12.0);
    ui.label(RichText::new(message).size(theme.text_small).color(theme.err));
    ui.add_space(8.0);

    let opts = Opts::secondary(vec2(128.0, widgets::combo::HEIGHT));
    button::label(ui, theme, t!("common.retry").as_ref(), opts)
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
    fn apply_page_appends_unique_and_advances() {
        let mut grid = PagedGrid { items: vec![movie(1, "A")], next_page: 2, ..PagedGrid::default() };
        let page = CatalogPage { items: vec![movie(1, "A"), movie(2, "B")], page: 2, total_pages: 5 };

        grid.apply_page(page);

        assert_eq!(grid.items.len(), 2);
        assert_eq!(grid.items[1].title, "B");
        assert_eq!(grid.next_page, 3);
        assert!(grid.has_more);
    }

    #[test]
    fn seeded_grid_resumes_at_page_two() {
        let grid = PagedGrid::seeded(vec![movie(1, "A")], Source::Remote);

        assert_eq!(grid.next_page, 2);
    }

    #[test]
    fn local_seed_does_not_page() {
        let grid = PagedGrid::seeded(vec![movie(1, "A")], Source::Local);

        assert!(!grid.has_more);
    }

    #[test]
    fn apply_page_stops_on_last_page() {
        let mut grid = PagedGrid { next_page: 3, ..PagedGrid::default() };
        let page = CatalogPage { items: vec![movie(9, "Z")], page: 3, total_pages: 3 };

        grid.apply_page(page);

        assert!(!grid.has_more);
        assert_eq!(grid.next_page, 4);
    }
}
