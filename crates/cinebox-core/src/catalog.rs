//! Home catalog rows and tiles.

use serde::{Deserialize, Serialize};

use crate::ids::{MediaKind, TmdbId};
use crate::settings::PosterSize;

const IMAGE_BASE: &str = "https://image.tmdb.org/t/p";

/// Build a TMDB image URL from a `poster_path` / `profile_path` / `backdrop_path`.
#[must_use]
pub fn tmdb_image_url(path: Option<&str>, size: &str) -> Option<String> {
    let path = normalize_tmdb_path(path)?;

    Some(format!(
        "{IMAGE_BASE}/{size}/{}",
        path.trim_start_matches('/')
    ))
}

/// Canonical TMDB file path (`/abc.jpg`), or `None` if empty.
#[must_use]
pub fn normalize_tmdb_path(path: Option<&str>) -> Option<String> {
    let path = path?.trim();
    if path.is_empty() {
        return None;
    }

    let path = path.trim_start_matches('/');
    Some(format!("/{path}"))
}

/// Split `https://image.tmdb.org/t/p/{size}/{path}` into size token and canonical path.
#[must_use]
pub fn parse_tmdb_image_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix(IMAGE_BASE)?.strip_prefix('/')?;
    let (size, path) = rest.split_once('/')?;
    if size.is_empty() || path.is_empty() {
        return None;
    }

    Some((size.to_owned(), format!("/{path}")))
}

/// A home-screen shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeRowId {
    RecentlyWatched,
    NowPlaying,
    TrendingDay,
    TrendingWeek,
    PopularMovies,
    PopularTv,
    TopRatedMovies,
    TopRatedTv,
}

impl HomeRowId {
    pub const ALL: [Self; 8] = [
        Self::RecentlyWatched,
        Self::NowPlaying,
        Self::TrendingDay,
        Self::TrendingWeek,
        Self::PopularMovies,
        Self::PopularTv,
        Self::TopRatedMovies,
        Self::TopRatedTv,
    ];

    /// Rows fetched from TMDB (not local history).
    pub const REMOTE: [Self; 7] = [
        Self::NowPlaying,
        Self::TrendingDay,
        Self::TrendingWeek,
        Self::PopularMovies,
        Self::PopularTv,
        Self::TopRatedMovies,
        Self::TopRatedTv,
    ];

    /// Stable cache key for this shelf.
    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::RecentlyWatched => "recently_watched",
            Self::NowPlaying => "now_playing",
            Self::TrendingDay => "trending_day",
            Self::TrendingWeek => "trending_week",
            Self::PopularMovies => "popular_movies",
            Self::PopularTv => "popular_tv",
            Self::TopRatedMovies => "top_rated_movies",
            Self::TopRatedTv => "top_rated_tv",
        }
    }

    #[must_use]
    pub const fn title_msg(self) -> crate::i18n::Msg {
        use crate::i18n::Msg;
        match self {
            Self::RecentlyWatched => Msg::HomeRecentlyWatched,
            Self::NowPlaying => Msg::HomeNowPlaying,
            Self::TrendingDay => Msg::HomeTrendingDay,
            Self::TrendingWeek => Msg::HomeTrendingWeek,
            Self::PopularMovies => Msg::HomePopularMovies,
            Self::PopularTv => Msg::HomePopularTv,
            Self::TopRatedMovies => Msg::HomeTopRatedMovies,
            Self::TopRatedTv => Msg::HomeTopRatedTv,
        }
    }

    #[must_use]
    pub const fn title(self) -> &'static str {
        self.title_msg().en()
    }

    /// TMDB list rows that can load extra pages. Local history cannot.
    #[must_use]
    pub const fn is_remote(self) -> bool {
        !matches!(self, Self::RecentlyWatched)
    }
}

/// One movie/TV tile on the home screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: TmdbId,
    pub kind: MediaKind,
    pub title: String,
    pub year: Option<u16>,
    pub vote: Option<f32>,
    pub poster_path: Option<String>,
}

impl CatalogItem {
    /// Full TMDB image URL, or `None` if there is no poster.
    #[must_use]
    pub fn poster_url(&self, size: PosterSize) -> Option<String> {
        tmdb_image_url(self.poster_path.as_deref(), size.tmdb_path())
    }
}

/// Parse `YYYY-MM-DD` (or any string starting with a year) into a year.
#[must_use]
pub fn year_from_date(date: &str) -> Option<u16> {
    date.get(..4)?.parse().ok()
}

/// One home shelf: items and/or a row-level error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeRow {
    pub id: HomeRowId,
    pub items: Vec<CatalogItem>,
    pub error: Option<String>,
}

impl HomeRow {
    #[must_use]
    pub fn empty(id: HomeRowId) -> Self {
        Self {
            id,
            items: Vec::new(),
            error: None,
        }
    }

    /// Poster paths on this shelf.
    #[must_use]
    pub fn image_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        for item in &self.items {
            let Some(path) = normalize_tmdb_path(item.poster_path.as_deref()) else {
                continue;
            };

            if paths.contains(&path) {
                continue;
            }

            paths.push(path);
        }
        paths
    }
}

/// Full home payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeCatalog {
    pub rows: Vec<HomeRow>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poster_url_uses_size_and_strips_slash() {
        let item = CatalogItem {
            id: TmdbId::new(1),
            kind: MediaKind::Movie,
            title: String::from("Test"),
            year: Some(2024),
            vote: Some(8.1),
            poster_path: Some(String::from("/abc.jpg")),
        };

        assert_eq!(
            item.poster_url(PosterSize::W342).as_deref(),
            Some("https://image.tmdb.org/t/p/w342/abc.jpg")
        );

        assert_eq!(
            parse_tmdb_image_url("https://image.tmdb.org/t/p/w500/abc.jpg"),
            Some((String::from("w500"), String::from("/abc.jpg")))
        );
    }

    #[test]
    fn year_parses_iso_date() {
        assert_eq!(year_from_date("2024-12-01"), Some(2024));
        assert_eq!(year_from_date(""), None);
    }

    #[test]
    fn remote_rows_match_tmdb_shelves() {
        assert!(!HomeRowId::RecentlyWatched.is_remote());

        for id in HomeRowId::REMOTE {
            assert!(id.is_remote());
        }
    }
}
