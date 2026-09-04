//! Shared models and settings persistence.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod db;
pub mod http;
pub mod ids;
pub mod media;
pub mod paths;
pub mod settings;

pub use catalog::{
    CatalogItem, HomeCatalog, HomeRow, HomeRowId, normalize_tmdb_path, parse_tmdb_image_url,
    tmdb_image_url, year_from_date,
};
pub use db::{
    CONFIG_TTL, CacheHit, DETAILS_TTL, KIND_CONFIG, KIND_HOME, KIND_MEDIA, KIND_PERSON,
    KIND_SEASON, RECENT_ROW_LIMIT, SEARCH_HISTORY_LIMIT, SEASON_TTL, Store, StoreError, TorrentPlaybackPrefs,
    WatchHistoryEntry, allowed_image_sizes, home_ttl, image_size_key, language_key,
    media_cache_id, media_kind_from_key, media_kind_key, media_ttl, person_cache_id,
    season_cache_id,
};
pub use http::{BaseUrlError, join_url, normalize_base_url};
pub use ids::{MediaKind, TmdbId};
pub use media::{
    CreditPerson, MediaDetails, PersonDetails, Trailer, decode_certification, format_money,
};
pub use settings::{
    GeneralSettings, ParserKind, ParserSettings, PlayerSettings, PosterSize, QualityBand,
    SecretString, Settings, SettingsError, SettingsStore, TorrServerSettings, UiLanguage,
    VideoScale,
};
pub use cinebox_typograf::typograph;
