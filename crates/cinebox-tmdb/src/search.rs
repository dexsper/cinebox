//! Name search HTTP (`/search/movie`, `/search/tv`, `/search/person`).

use cinebox_core::{CatalogItem, MediaKind};
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

/// One page of a name search; `page` starts at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchQuery<'a> {
    pub text: &'a str,
    pub kind: SearchKind,
    pub page: u32,
}

/// A movie or show a name search found, with the title it was released under.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleMatch {
    pub item: CatalogItem,
    pub original_title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    page: Option<u32>,
    total_pages: Option<u32>,
    results: Option<Vec<CatalogListItem>>,
}

/// One TMDB search page. Empty / whitespace queries skip the network.
///
/// # Errors
///
/// Empty key, HTTP client build failure, or a TMDB HTTP/JSON error.
pub(crate) async fn fetch_search_page(
    api_key: &str,
    search: SearchQuery<'_>,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<CatalogPage, Error> {
    let SearchQuery { text: query, kind, page } = search;
    let page = page.max(1);
    if query.trim().is_empty() {
        return Ok(CatalogPage { items: Vec::new(), page, total_pages: 1 });
    }

    let parsed = fetch_list(api_key, search, language, net).await?;
    let raw = parsed.results.unwrap_or_default();
    let items = match kind {
        SearchKind::Person => person_items_from(raw, MAX_ROW_ITEMS),
        SearchKind::Movie | SearchKind::Tv => catalog_items_from(raw, kind.fallback_kind(), MAX_ROW_ITEMS),
    };

    let (page, total_pages) = page_bounds(page, parsed.page, parsed.total_pages, items.len());

    Ok(CatalogPage { items, page, total_pages })
}

/// First-page movie or show results for `search` with their original titles. A person search and an empty
/// query find nothing.
pub(crate) async fn fetch_title_matches(
    api_key: &str,
    search: SearchQuery<'_>,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<Vec<TitleMatch>, Error> {
    let Some(fallback) = search.kind.fallback_kind() else { return Ok(Vec::new()) };

    if search.text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let parsed = fetch_list(api_key, search, language, net).await?;
    let raw = parsed.results.unwrap_or_default();
    let mut matches = Vec::new();
    for row in raw {
        let original_title = row.original_title.clone().or_else(|| row.original_name.clone());
        let Some(item) = catalog_items_from([row], Some(fallback), 1).pop() else {
            continue;
        };

        matches.push(TitleMatch { item, original_title });
    }

    Ok(matches)
}

async fn fetch_list(
    api_key: &str,
    search: SearchQuery<'_>,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<ListResponse, Error> {
    let api_key = crate::prepare_api_key(api_key)?.to_owned();
    let page_s = search.page.max(1).to_string();
    let url = format!("{API_BASE}/{}", search.kind.path());
    let language = language.filter(|s| !s.is_empty()).map(str::to_owned);
    let query = search.text.trim().to_owned();

    send_json(net, |client| {
        let mut request = client.get(&url).timeout(std::time::Duration::from_secs(20)).query(&[
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
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_query_skips_network() {
        let net = NetConfig::direct();
        let search = SearchQuery { text: "  ", kind: SearchKind::Movie, page: 1 };
        let result = fetch_search_page("key", search, None, &net).await;
        let Ok(page) = result else {
            panic!("empty query should skip the network");
        };

        assert_eq!(page.items, []);
        assert_eq!(page.page, 1);
        assert_eq!(page.total_pages, 1);
    }

    #[tokio::test]
    async fn title_matches_skip_people_and_empty_queries() {
        let net = NetConfig::direct();
        let people = SearchQuery { text: "Dune", kind: SearchKind::Person, page: 1 };
        let empty = SearchQuery { text: " ", kind: SearchKind::Tv, page: 1 };

        let found_people = fetch_title_matches("key", people, None, &net).await;
        let found_empty = fetch_title_matches("key", empty, None, &net).await;

        assert!(found_people.is_ok_and(|found| found.is_empty()));
        assert!(found_empty.is_ok_and(|found| found.is_empty()));
    }
}
