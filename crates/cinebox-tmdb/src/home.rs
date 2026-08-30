//! Home-row HTTP. Catalog endpoints.

use cinebox_core::{HomeCatalog, HomeRow, HomeRowId, MediaKind};
use futures_util::future::join_all;
use serde::Deserialize;

use crate::catalog_map::{CatalogListItem, catalog_items_from};
use crate::{Error, http_client, send_json};

const API_BASE: &str = crate::API_BASE;
pub const MAX_ROW_ITEMS: usize = 20;

#[derive(Debug, Deserialize)]
struct ListResponse {
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

async fn fetch_row(
    client: reqwest::Client,
    api_key: String,
    language: Option<String>,
    id: HomeRowId,
) -> HomeRow {
    let Some(path) = path_for(id) else {
        return HomeRow::empty(id);
    };
    let url = format!("{API_BASE}/{path}");
    let mut request = client.get(&url).query(&[("api_key", api_key.as_str())]);
    if let Some(language) = language.as_deref().filter(|s| !s.is_empty()) {
        request = request.query(&[("language", language)]);
    }
    match send_list(request).await {
        Ok(items) => HomeRow {
            id,
            items: catalog_items_from(items, default_kind(id), MAX_ROW_ITEMS),
            error: None,
        },
        Err(error) => HomeRow {
            id,
            items: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

async fn send_list(request: reqwest::RequestBuilder) -> Result<Vec<CatalogListItem>, Error> {
    let parsed: ListResponse = send_json(request).await?;
    Ok(parsed.results.unwrap_or_default())
}

pub async fn fetch_home(
    api_key: &str,
    language: Option<&str>,
    use_system_proxy: bool,
) -> Result<HomeCatalog, Error> {
    let api_key = crate::prepare_api_key(api_key)?;
    let client = http_client(std::time::Duration::from_secs(20), use_system_proxy)?;
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
