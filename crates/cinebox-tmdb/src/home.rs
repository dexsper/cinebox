//! Home-row HTTP. Catalog endpoints.

use cinebox_core::{CatalogItem, HomeCatalog, HomeRow, HomeRowId, MediaKind};
use futures_util::future::join_all;
use serde::Deserialize;

use crate::catalog_map::{CatalogListItem, catalog_items_from};
use crate::{Error, http_client, send_json};

const API_BASE: &str = crate::API_BASE;
pub const MAX_ROW_ITEMS: usize = 20;

/// One TMDB list page for a home shelf.
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

fn path_for(id: HomeRowId) -> Option<&'static str> {
    match id {
        HomeRowId::RecentlyWatched => None,
        HomeRowId::NowPlaying => Some("movie/now_playing"),
        HomeRowId::TrendingDay => Some("trending/all/day"),
        HomeRowId::TrendingWeek => Some("trending/all/week"),
        HomeRowId::PopularMovies => Some("movie/popular"),
        HomeRowId::PopularTv => Some("tv/popular"),
        HomeRowId::TopRatedMovies => Some("movie/top_rated"),
        HomeRowId::TopRatedTv => Some("tv/top_rated"),
    }
}

fn default_kind(id: HomeRowId) -> Option<MediaKind> {
    match id {
        HomeRowId::RecentlyWatched | HomeRowId::TrendingDay | HomeRowId::TrendingWeek => None,
        HomeRowId::NowPlaying | HomeRowId::PopularMovies | HomeRowId::TopRatedMovies => {
            Some(MediaKind::Movie)
        }
        HomeRowId::PopularTv | HomeRowId::TopRatedTv => Some(MediaKind::Tv),
    }
}

fn page_bounds(
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

async fn fetch_page(
    client: reqwest::Client,
    api_key: String,
    language: Option<String>,
    id: HomeRowId,
    page: u32,
) -> Result<CatalogPage, Error> {
    let Some(path) = path_for(id) else {
        return Ok(CatalogPage {
            items: Vec::new(),
            page: page.max(1),
            total_pages: 1,
        });
    };

    let url = format!("{API_BASE}/{path}");
    let page = page.max(1);
    let page_s = page.to_string();
    let mut request = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .query(&[("api_key", api_key.as_str()), ("page", page_s.as_str())]);

    if let Some(language) = language.as_deref().filter(|s| !s.is_empty()) {
        request = request.query(&[("language", language)]);
    }

    let parsed: ListResponse = send_json(request).await?;
    let raw = parsed.results.unwrap_or_default();
    let items = catalog_items_from(raw, default_kind(id), MAX_ROW_ITEMS);
    let (page, total_pages) = page_bounds(page, parsed.page, parsed.total_pages, items.len());

    Ok(CatalogPage {
        items,
        page,
        total_pages,
    })
}

async fn fetch_row(
    client: reqwest::Client,
    api_key: String,
    language: Option<String>,
    id: HomeRowId,
) -> HomeRow {
    match fetch_page(client, api_key, language, id, 1).await {
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
    use_system_proxy: bool,
) -> Result<HomeCatalog, Error> {
    let api_key = crate::prepare_api_key(api_key)?;
    let client = http_client(use_system_proxy)?;
    let language = language.map(str::to_owned);
    let futs = HomeRowId::REMOTE.into_iter().map(|id| {
        let client = client.clone();
        let key = api_key.to_owned();
        let language = language.clone();
        async move { fetch_row(client, key, language, id).await }
    });
    
    let mut rows = Vec::with_capacity(HomeRowId::ALL.len());
    rows.push(HomeRow::empty(HomeRowId::RecentlyWatched));
    rows.extend(join_all(futs).await);
    Ok(HomeCatalog { rows })
}

pub async fn fetch_catalog_page(
    api_key: &str,
    id: HomeRowId,
    page: u32,
    language: Option<&str>,
    use_system_proxy: bool,
) -> Result<CatalogPage, Error> {
    if path_for(id).is_none() {
        return Ok(CatalogPage {
            items: Vec::new(),
            page: page.max(1),
            total_pages: 1,
        });
    }

    let api_key = crate::prepare_api_key(api_key)?;
    let client = http_client(use_system_proxy)?;
    let language = language.map(str::to_owned);

    fetch_page(client, api_key.to_owned(), language, id, page).await
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
