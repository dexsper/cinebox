//! Cache keys, TTLs, and row types for the local SQLite store.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::catalog::{CatalogItem, HomeRowId};
use crate::ids::{MediaKind, TmdbId};
use crate::media::MediaDetails;
use crate::settings::{PosterSize, VideoScale};

/// TMDB content older than this must be dropped (API ToS).
pub const MAX_AGE: Duration = Duration::from_secs(183 * 24 * 3600);
/// Home `now_playing` / `trending/day`.
pub const HOME_FAST_TTL: Duration = Duration::from_secs(3 * 3600);
/// Home popular / top rated / trending week.
pub const HOME_SLOW_TTL: Duration = Duration::from_secs(18 * 3600);
/// Media / person / in-progress TV seasons.
pub const DETAILS_TTL: Duration = Duration::from_secs(24 * 3600);
/// Released movie details.
pub const DETAILS_STABLE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
/// Season episode lists.
pub const SEASON_TTL: Duration = Duration::from_secs(24 * 3600);
/// `/configuration` API-key probe.
pub const CONFIG_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

pub(crate) const IMAGE_GC_GRACE: Duration = Duration::from_secs(5 * 60);
pub(crate) const IMAGE_BUDGET_BYTES: i64 = 512 * 1024 * 1024;

pub const KIND_HOME: &str = "home";
pub const KIND_MEDIA: &str = "media";
pub const KIND_PERSON: &str = "person";
pub const KIND_SEASON: &str = "season";
pub const KIND_CONFIG: &str = "config";

/// Home shelf and query cap for local watch history.
pub const RECENT_ROW_LIMIT: usize = 20;
/// Recent torrent hashes kept per movie/show (shared TV, multiple seasons).
pub const RECENT_RELEASE_LIMIT: usize = 3;

/// Per-torrent playback preferences, stored as one JSON payload by hash.
///
/// `aid` / `sid` are mpv track ids. `-1` means "auto" (leave mpv's pick alone);
/// `sid == 0` means subtitles explicitly off.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TorrentPlaybackPrefs {
    pub scale: VideoScale,
    pub speed: f64,
    pub aid: i64,
    pub sid: i64,
}

impl Default for TorrentPlaybackPrefs {
    fn default() -> Self {
        Self {
            scale: VideoScale::default(),
            speed: 1.0,
            aid: -1,
            sid: -1,
        }
    }
}

/// One row of local watch history: a movie or show the user started playing.
///
/// `season` / `episode` point at the last-watched episode (`None` for movies);
/// `time` / `duration` are that episode's position, seconds. The display
/// fields are a denormalized snapshot so the Home shelf never depends on the
/// TMDB cache staying warm.
#[derive(Debug, Clone, PartialEq)]
pub struct WatchHistoryEntry {
    pub kind: MediaKind,
    pub id: TmdbId,
    pub title: String,
    pub poster_path: Option<String>,
    pub year: Option<u16>,
    pub vote: Option<f32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub episode_title: Option<String>,
    pub time: f64,
    pub duration: f64,
}

impl WatchHistoryEntry {
    /// Poster tile for the Home shelf.
    #[must_use]
    pub fn as_catalog_item(&self) -> CatalogItem {
        CatalogItem {
            id: self.id,
            kind: self.kind,
            title: self.title.clone(),
            year: self.year,
            vote: self.vote,
            poster_path: self.poster_path.clone(),
        }
    }
}

/// Cached JSON plus when it was fetched (unix seconds).
#[derive(Debug, Clone)]
pub struct CacheHit<T> {
    pub value: T,
    pub fetched_at: i64,
}

impl<T> CacheHit<T> {
    #[must_use]
    pub fn is_fresh(&self, ttl: Duration) -> bool {
        age_secs(self.fetched_at) < i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX)
    }
}

/// `watch_timeline` season/episode column value: `-1` marks a movie.
pub(crate) fn episode_key(value: Option<u32>) -> i64 {
    value.map_or(-1, i64::from)
}

pub(crate) fn unix_now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    )
    .unwrap_or(0)
}

pub(crate) fn age_secs(fetched_at: i64) -> i64 {
    unix_now().saturating_sub(fetched_at)
}

/// Fresh TTL for a home shelf.
#[must_use]
pub const fn home_ttl(id: HomeRowId) -> Duration {
    match id {
        HomeRowId::NowPlaying | HomeRowId::TrendingDay => HOME_FAST_TTL,
        _ => HOME_SLOW_TTL,
    }
}

/// Fresh TTL for a media card.
#[must_use]
pub fn media_ttl(details: &MediaDetails) -> Duration {
    let has_release_date = details.released.as_ref().is_some_and(|s| !s.is_empty());

    if details.kind == MediaKind::Movie && has_release_date {
        return DETAILS_STABLE_TTL;
    }

    DETAILS_TTL
}

/// SQL text for a media kind column.
#[must_use]
pub const fn media_kind_key(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "movie",
        MediaKind::Tv => "tv",
        MediaKind::Person => "person",
    }
}

/// Parse a media kind stored by [`media_kind_key`].
#[must_use]
pub fn media_kind_from_key(key: &str) -> Option<MediaKind> {
    match key {
        "movie" => Some(MediaKind::Movie),
        "tv" => Some(MediaKind::Tv),
        "person" => Some(MediaKind::Person),
        _ => None,
    }
}

/// Cache id for a movie/TV card.
#[must_use]
pub fn media_cache_id(kind: MediaKind, id: TmdbId) -> String {
    format!("{}:{}", media_kind_key(kind), id.get())
}

/// Cache id for a person page.
#[must_use]
pub fn person_cache_id(id: TmdbId) -> String {
    id.get().to_string()
}

/// Cache id for one TV season.
#[must_use]
pub fn season_cache_id(tv_id: TmdbId, season: u32) -> String {
    format!("{}:{season}", tv_id.get())
}

/// Language column value (`""` when unset).
#[must_use]
pub fn language_key(language: Option<&str>) -> &str {
    language.filter(|s| !s.is_empty()).unwrap_or("")
}

/// Image size tokens that must stay after GC.
#[must_use]
pub fn allowed_image_sizes(poster: PosterSize) -> Vec<String> {
    vec![
        poster.tmdb_path().to_owned(),
        String::from("w185"),
        String::from("w1280"),
        String::from("w1280~soft"),
        String::from("w300"),
    ]
}

/// Size column for an image URL, including the soften suffix.
#[must_use]
pub fn image_size_key(size: &str, soften: bool) -> String {
    if soften {
        return format!("{size}~soft");
    }

    size.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details(kind: MediaKind, released: Option<&str>) -> MediaDetails {
        MediaDetails {
            id: TmdbId::new(1),
            kind,
            title: String::from("T"),
            original_title: None,
            original_language: None,
            tagline: None,
            overview: None,
            year: None,
            released: released.map(str::to_owned),
            runtime_minutes: None,
            number_of_seasons: None,
            number_of_episodes: None,
            certification: None,
            vote: None,
            budget: None,
            genre_ids: Vec::new(),
            genres: Vec::new(),
            countries: Vec::new(),
            poster_path: None,
            backdrop_path: None,
            directors: Vec::new(),
            cast: Vec::new(),
            collection: Vec::new(),
            recommendations: Vec::new(),
            similar: Vec::new(),
            trailers: Vec::new(),
        }
    }

    #[test]
    fn released_movie_gets_stable_ttl() {
        let released = details(MediaKind::Movie, Some("2024-01-01"));
        assert_eq!(media_ttl(&released), DETAILS_STABLE_TTL);
    }

    #[test]
    fn unreleased_movie_and_tv_get_short_ttl() {
        assert_eq!(media_ttl(&details(MediaKind::Movie, None)), DETAILS_TTL);
        assert_eq!(media_ttl(&details(MediaKind::Movie, Some(""))), DETAILS_TTL);
        assert_eq!(
            media_ttl(&details(MediaKind::Tv, Some("2024-01-01"))),
            DETAILS_TTL
        );
    }

    #[test]
    fn fast_shelves_get_fast_ttl() {
        assert_eq!(home_ttl(HomeRowId::NowPlaying), HOME_FAST_TTL);
        assert_eq!(home_ttl(HomeRowId::TrendingDay), HOME_FAST_TTL);
        assert_eq!(home_ttl(HomeRowId::PopularMovies), HOME_SLOW_TTL);
        assert_eq!(home_ttl(HomeRowId::TopRatedTv), HOME_SLOW_TTL);
    }

    #[test]
    fn cache_hit_freshness_uses_ttl() {
        let now = unix_now();
        let fresh = CacheHit {
            value: (),
            fetched_at: now,
        };

        assert!(fresh.is_fresh(Duration::from_secs(60)));
        let stale = CacheHit {
            value: (),
            fetched_at: now - 120,
        };

        assert!(!stale.is_fresh(Duration::from_secs(60)));
    }
}
