//! Home-row HTTP. Endpoints match the ТЗ / Lampa catalog.

use cinebox_core::{
    CatalogItem, HomeCatalog, HomeRow, HomeRowId, MediaKind, TmdbId, year_from_date,
};
use futures_util::future::join_all;
use serde::Deserialize;

use crate::{Error, http_client};

const API_BASE: &str = "https://api.themoviedb.org/3";
pub const MAX_ROW_ITEMS: usize = 20;

#[derive(Debug, Deserialize)]
struct ListResponse {
    results: Option<Vec<ListItem>>,
}

#[derive(Debug, Deserialize)]
struct ListItem {
    id: Option<u32>,
    title: Option<String>,
    name: Option<String>,
    poster_path: Option<String>,
    vote_average: Option<f32>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    media_type: Option<String>,
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

fn items_from_list(
    items: &[ListItem],
    fallback_kind: Option<MediaKind>,
) -> Vec<CatalogItem> {
    let mut out = Vec::with_capacity(items.len().min(MAX_ROW_ITEMS));
    for raw in items {
        if out.len() >= MAX_ROW_ITEMS {
            break;
        }
        let Some(id) = raw.id.filter(|id| *id > 0) else {
            continue;
        };
        let kind = match raw
            .media_type
            .as_deref()
            .and_then(kind_from_media_type)
            .or(fallback_kind)
        {
            Some(kind @ (MediaKind::Movie | MediaKind::Tv)) => kind,
            _ => continue,
        };
        let title = match kind {
            MediaKind::Movie => raw.title.clone().or_else(|| raw.name.clone()),
            MediaKind::Tv => raw.name.clone().or_else(|| raw.title.clone()),
            MediaKind::Person => continue,
        };
        let Some(title) = title.filter(|t| !t.is_empty()) else {
            continue;
        };
        let date = match kind {
            MediaKind::Movie => raw.release_date.as_deref(),
            MediaKind::Tv => raw.first_air_date.as_deref(),
            MediaKind::Person => None,
        };
        let vote = raw.vote_average.filter(|v| *v > 0.0);
        out.push(CatalogItem {
            id: TmdbId::new(id),
            kind,
            title,
            year: date.and_then(year_from_date),
            vote,
            poster_path: raw.poster_path.clone().filter(|p| !p.is_empty()),
        });
    }
    out
}

fn kind_from_media_type(value: &str) -> Option<MediaKind> {
    match value {
        "movie" => Some(MediaKind::Movie),
        "tv" => Some(MediaKind::Tv),
        "person" => Some(MediaKind::Person),
        _ => None,
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
            items: items_from_list(&items, default_kind(id)),
            error: None,
        },
        Err(error) => HomeRow {
            id,
            items: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

async fn send_list(request: reqwest::RequestBuilder) -> Result<Vec<ListItem>, Error> {
    let response = request.send().await.map_err(crate::into_request)?;
    let status = response.status();
    if status.as_u16() == 401 {
        return Err(Error::Unauthorized);
    }
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }
    let parsed: ListResponse = response.json().await.map_err(crate::into_request)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_trending_and_skips_people() {
        let items = vec![
            ListItem {
                id: Some(10),
                title: Some(String::from("Film")),
                name: None,
                poster_path: Some(String::from("/p.jpg")),
                vote_average: Some(7.5),
                release_date: Some(String::from("2023-01-02")),
                first_air_date: None,
                media_type: Some(String::from("movie")),
            },
            ListItem {
                id: Some(11),
                title: None,
                name: Some(String::from("Show")),
                poster_path: None,
                vote_average: Some(8.0),
                release_date: None,
                first_air_date: Some(String::from("2022-05-01")),
                media_type: Some(String::from("tv")),
            },
            ListItem {
                id: Some(12),
                title: None,
                name: Some(String::from("Actor")),
                poster_path: None,
                vote_average: None,
                release_date: None,
                first_air_date: None,
                media_type: Some(String::from("person")),
            },
        ];
        let parsed = items_from_list(&items, None);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].kind, MediaKind::Movie);
        assert_eq!(parsed[0].year, Some(2023));
        assert_eq!(parsed[1].kind, MediaKind::Tv);
        assert_eq!(parsed[1].title, "Show");
    }
}
