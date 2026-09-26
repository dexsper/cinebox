//! Shared poster-grid pagination used by category, discover, and search screens.

use std::collections::HashSet;
use std::future::Future;

use cinebox_core::CatalogItem;
use cinebox_tmdb::CatalogPage;
use egui::{AsIdSalt, RichText, Sense, Ui, vec2};
use egui_async::Bind;
use rust_i18n::t;

use crate::jobs::JobError;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, poster, scroll};

/// Infinite poster grid over one TMDB list.
pub struct PagedGrid {
    pub items: Vec<CatalogItem>,
    next_page: u32,
    has_more: bool,
    loading: bool,
    page: Bind<CatalogPage, JobError>,
}

impl Default for PagedGrid {
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

pub struct GridOut {
    pub action: Option<NavAction>,
    /// Caller should [`PagedGrid::request`] the next page.
    pub wants_page: bool,
}

impl PagedGrid {
    /// Start from items already shown elsewhere (a Home shelf). `remote` lists page on.
    pub fn seeded(items: Vec<CatalogItem>, remote: bool) -> Self {
        let next_page = if items.is_empty() { 1 } else { 2 };

        Self {
            items,
            next_page,
            has_more: remote,
            ..Self::default()
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Start loading the next page unless one is already in flight.
    pub fn request<Fut>(&mut self, load: impl FnOnce(u32) -> Fut)
    where
        Fut: Future<Output = Result<CatalogPage, JobError>> + Send + 'static,
    {
        if self.loading || !self.has_more {
            return;
        }

        self.loading = true;
        let page = self.next_page;
        let _ = self.page.read_or_request(|| load(page));
    }

    /// Header, then the grid (or its loading, error, or empty state), all in one scroll.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        svc: &Services,
        theme: &Theme,
        scroll_salt: impl AsIdSalt,
        to_top: bool,
        header: impl FnOnce(&mut Ui),
    ) -> GridOut {
        self.take_page();

        let error = match self.page.read() {
            Some(Err(error)) => Some(error.to_string()),
            _ => None,
        };
        let mut near_end = false;
        let mut action = None;
        let mut retry = false;

        let body = |ui: &mut Ui| {
            header(ui);

            let scale = poster::card_scale(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                for item in &self.items {
                    let marks = svc.tile_marks(item.kind, item.id);
                    let size = svc.settings.tmdb.poster_size;
                    let opened = poster::catalog_tile(ui, item, &svc.images, size, theme, marks, scale);
                    if action.is_none() {
                        action = opened;
                    }
                }
            });

            let sentinel_size = vec2(ui.available_width(), 1.0);
            let (sentinel, _) = ui.allocate_exact_size(sentinel_size, Sense::hover());
            near_end = poster::in_load_window(ui, sentinel);

            if let Some(error) = &error {
                ui.add_space(12.0);
                ui.label(RichText::new(error).size(theme.text_small).color(theme.err));
                ui.add_space(8.0);
                retry = widgets::button::label(
                    ui,
                    theme,
                    t!("common.retry").as_ref(),
                    widgets::button::Opts::secondary(vec2(128.0, widgets::combo::HEIGHT)),
                );
                return;
            }

            if self.loading || (self.items.is_empty() && self.has_more) {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(28.0).color(theme.muted));
                });
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
        };

        if to_top {
            scroll::vertical_to_top(ui, scroll_salt, body);
        } else {
            scroll::vertical(ui, scroll_salt, body);
        }

        if retry {
            self.page.clear();
            self.loading = false;
        }

        if self.loading {
            ui.ctx().request_repaint();
        }

        let wants_more = self.items.is_empty() || near_end;
        GridOut {
            action,
            wants_page: wants_more && self.has_more && !self.loading && error.is_none() && !retry,
        }
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
        apply_page(&mut self.items, &mut self.next_page, &mut self.has_more, page);
    }
}

pub fn apply_page(
    items: &mut Vec<CatalogItem>,
    next_page: &mut u32,
    has_more: &mut bool,
    page: CatalogPage,
) {
    extend_unique(items, page.items);

    let seen = page.page.max(*next_page);
    *next_page = seen.saturating_add(1);
    *has_more = page.page < page.total_pages;
}

pub fn extend_unique(items: &mut Vec<CatalogItem>, incoming: Vec<CatalogItem>) {
    let mut seen: HashSet<_> = items.iter().map(|item| (item.id, item.kind)).collect();

    for item in incoming {
        let inserted = seen.insert((item.id, item.kind));
        if !inserted {
            continue;
        }

        items.push(item);
    }
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
        let mut items = vec![movie(1, "A")];
        let mut next_page = 2;
        let mut has_more = true;
        let page = CatalogPage {
            items: vec![movie(1, "A"), movie(2, "B")],
            page: 2,
            total_pages: 5,
        };

        apply_page(&mut items, &mut next_page, &mut has_more, page);

        assert_eq!(items.len(), 2);
        assert_eq!(items[1].title, "B");
        assert_eq!(next_page, 3);
        assert!(has_more);
    }

    #[test]
    fn seeded_grid_resumes_at_page_two() {
        let grid = PagedGrid::seeded(vec![movie(1, "A")], true);

        assert_eq!(grid.next_page, 2);
    }

    #[test]
    fn local_seed_does_not_page() {
        let grid = PagedGrid::seeded(vec![movie(1, "A")], false);

        assert!(!grid.has_more);
    }

    #[test]
    fn apply_page_stops_on_last_page() {
        let mut items = Vec::new();
        let mut next_page = 3;
        let mut has_more = true;
        let page = CatalogPage {
            items: vec![movie(9, "Z")],
            page: 3,
            total_pages: 3,
        };

        apply_page(&mut items, &mut next_page, &mut has_more, page);

        assert!(!has_more);
        assert_eq!(next_page, 4);
    }
}
