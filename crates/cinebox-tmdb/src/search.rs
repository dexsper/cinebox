//! Name search HTTP (`/search/movie`, `/search/tv`, `/search/person`).

use cinebox_core::MediaKind;
use cinebox_net::NetConfig;
use serde::Deserialize;

use crate::catalog_map::{CatalogListItem, catalog_items_from, person_items_from};
use crate::home::{CatalogPage, MAX_ROW_ITEMS, page_bounds};
use crate::{API_BASE, Error, send_json};

/// Which TMDB search endpoint to hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Movie,
    Tv,
    Person,
}

impl SearchKind {
    fn path(self) -> &'static str {
        match self {
            Self::Movie => "search/movie",
            Self::Tv => "search/tv",
            Self::Person => "search/person",
        }
    }

    fn fallback_kind(self) -> Option<MediaKind> {
        match self {
            Self::Movie => Some(MediaKind::Movie),
            Self::Tv => Some(MediaKind::Tv),
            Self::Person => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    page: Option<u32>,
    total_pages: Option<u32>,
    results: Option<Vec<CatalogListItem>>,
}

/// One TMDB search page (`page` starts at 1). Empty / whitespace queries skip the network.
///
/// # Errors
///
/// Empty key, HTTP client build failure, or a TMDB HTTP/JSON error.
pub async fn fetch_search_page(
    api_key: &str,
    query: &str,
    kind: SearchKind,
    page: u32,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<CatalogPage, Error> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(CatalogPage {
            items: Vec::new(),
            page: page.max(1),
            total_pages: 1,
        });
    }

    let api_key = crate::prepare_api_key(api_key)?.to_owned();
    let page = page.max(1);
    let page_s = page.to_string();
    let path = kind.path();
    let url = format!("{API_BASE}/{path}");
    let language = language.filter(|s| !s.is_empty()).map(str::to_owned);
    let query = query.to_owned();

    let parsed: ListResponse = send_json(net, |client| {
        let mut request = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(20))
            .query(&[
                ("api_key", api_key.as_str()),
                ("query", query.as_str()),
                ("page", page_s.as_str()),
                ("include_adult", "false"),
            ]);

        if let Some(language) = language.as_deref() {
            request = request.query(&[("language", language)]);
        }

        request
    })
    .await?;

    let raw = parsed.results.unwrap_or_default();
    let items = match kind {
        SearchKind::Person => person_items_from(raw, MAX_ROW_ITEMS),
        SearchKind::Movie | SearchKind::Tv => {
            catalog_items_from(raw, kind.fallback_kind(), MAX_ROW_ITEMS)
        }
    };

    let (page, total_pages) = page_bounds(page, parsed.page, parsed.total_pages, items.len());

    Ok(CatalogPage {
        items,
        page,
        total_pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_query_skips_network() {
        let net = NetConfig::direct();
        let result = fetch_search_page("key", "  ", SearchKind::Movie, 1, None, &net).await;
        let Ok(page) = result else {
            panic!("empty query should skip the network");
        };

        assert!(page.items.is_empty());
        assert_eq!(page.page, 1);
        assert_eq!(page.total_pages, 1);
    }
}
