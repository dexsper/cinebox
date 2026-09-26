//! Home-row HTTP and the shared list-endpoint fetcher.

use cinebox_core::{CatalogItem, HomeCatalog, HomeRow, HomeRowId, MediaKind};
use cinebox_net::NetConfig;
use futures_util::future::join_all;
use serde::Deserialize;

use crate::catalog_map::{CatalogListItem, catalog_items_from};
use crate::discover::{Date, DiscoverQuery};
use crate::shelves::{ShelfId, ShelfSource, fetch_source_page};
use crate::{Error, send_json};

const API_BASE: &str = crate::API_BASE;
pub const MAX_ROW_ITEMS: usize = 20;

/// One TMDB list page for a shelf.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogPage {
    pub items: Vec<CatalogItem>,
    pub page: u32,
    pub total_pages: u32,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    page: Option<u32>,
    total_pages: Option<u32>,
    results: Option<Vec<CatalogListItem>>,
}

/// One paged list endpoint such as `movie/popular` or `discover/tv`.
pub(crate) struct ListRequest<'a> {
    pub(crate) path: &'a str,
    pub(crate) params: &'a [(&'static str, String)],
    /// Kind for results without `media_type`.
    pub(crate) kind: Option<MediaKind>,
    pub(crate) exclude_language: Option<&'a str>,
}

pub(crate) fn page_bounds(
    requested: u32,
    page: Option<u32>,
    total_pages: Option<u32>,
    item_count: usize,
) -> (u32, u32) {
    let page = page.unwrap_or(requested).max(1);

    if let Some(total) = total_pages.filter(|n| *n > 0) {
        return (page, total);
    }

    if item_count >= MAX_ROW_ITEMS {
        return (page, page.saturating_add(1));
    }

    (page, page)
}

pub(crate) async fn fetch_list(
    net: &NetConfig,
    api_key: &str,
    language: Option<&str>,
    request: &ListRequest<'_>,
    page: u32,
) -> Result<CatalogPage, Error> {
    let url = format!("{API_BASE}/{}", request.path);
    let page = page.max(1);
    let page_s = page.to_string();

    let parsed: ListResponse = send_json(net, |client| {
        let mut builder = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(20))
            .query(&[("api_key", api_key), ("page", page_s.as_str())])
            .query(request.params);

        if let Some(language) = language.filter(|s| !s.is_empty()) {
            builder = builder.query(&[("language", language)]);
        }

        builder
    })
    .await?;

    let raw: Vec<CatalogListItem> = parsed
        .results
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            request.exclude_language.is_none()
                || item.original_language.as_deref() != request.exclude_language
        })
        .collect();

    let items = catalog_items_from(raw, request.kind, MAX_ROW_ITEMS);
    let (page, total_pages) = page_bounds(page, parsed.page, parsed.total_pages, items.len());

    Ok(CatalogPage {
        items,
        page,
        total_pages,
    })
}

async fn fetch_row(
    net: &NetConfig,
    api_key: &str,
    language: Option<&str>,
    id: HomeRowId,
    today: Date,
) -> HomeRow {
    let source = ShelfId::Home(id).source(today);
    match fetch_source_page(net, api_key, language, &source, 1, today).await {
        Ok(page) => HomeRow {
            id,
            items: page.items,
            error: None,
        },
        Err(error) => HomeRow {
            id,
            items: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

pub async fn fetch_home(
    api_key: &str,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<HomeCatalog, Error> {
    let api_key = crate::prepare_api_key(api_key)?;
    let today = Date::today();
    let futs = HomeRowId::REMOTE
        .into_iter()
        .map(|id| fetch_row(net, api_key, language, id, today));

    let mut rows = Vec::with_capacity(HomeRowId::ALL.len());
    rows.extend(HomeRowId::LOCAL.into_iter().map(HomeRow::empty));
    rows.extend(join_all(futs).await);

    Ok(HomeCatalog { rows })
}

pub async fn fetch_catalog_page(
    api_key: &str,
    id: ShelfId,
    page: u32,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<CatalogPage, Error> {
    let today = Date::today();
    let source = id.source(today);
    if source == ShelfSource::Local {
        return Ok(CatalogPage {
            items: Vec::new(),
            page: page.max(1),
            total_pages: 1,
        });
    }

    let api_key = crate::prepare_api_key(api_key)?;
    fetch_source_page(net, api_key, language, &source, page, today).await
}

pub async fn fetch_discover_page(
    api_key: &str,
    query: &DiscoverQuery,
    page: u32,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<CatalogPage, Error> {
    let api_key = crate::prepare_api_key(api_key)?;
    let source = ShelfSource::Discover(Box::new(query.clone()));

    fetch_source_page(net, api_key, language, &source, page, Date::today()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_bounds_prefers_tmdb_total() {
        assert_eq!(page_bounds(2, Some(2), Some(40), 20), (2, 40));
        assert_eq!(page_bounds(1, None, Some(0), 20), (1, 2));
        assert_eq!(page_bounds(3, None, None, 4), (3, 3));
        assert_eq!(page_bounds(1, None, None, 20), (1, 2));
    }
}
