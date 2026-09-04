//! Shared poster-grid pagination used by category and search screens.

use std::collections::HashSet;

use cinebox_core::CatalogItem;
use cinebox_tmdb::CatalogPage;

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
