//! Shared models, settings persistence, and i18n keys.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod db;
pub mod http;
pub mod i18n;
pub mod ids;
pub mod media;
pub mod paths;
pub mod settings;
pub mod typograf;

pub use catalog::{
    CatalogItem, HomeCatalog, HomeRow, HomeRowId, normalize_tmdb_path, parse_tmdb_image_url,
    tmdb_image_url, year_from_date,
};
pub use db::{
    CONFIG_TTL, CacheHit, DETAILS_TTL, KIND_CONFIG, KIND_HOME, KIND_MEDIA, KIND_PERSON,
    KIND_SEASON, SEASON_TTL, Store, StoreError, allowed_image_sizes, home_ttl, image_size_key,
    language_key, media_cache_id, media_ttl, person_cache_id, season_cache_id,
};
pub use http::{BaseUrlError, join_url, normalize_base_url};
pub use ids::{MediaKind, TmdbId};
pub use media::{
    CreditPerson, MediaDetails, PersonDetails, Trailer, decode_certification, format_money,
    format_release_date, format_runtime,
};
pub use settings::{
    GeneralSettings, ParserKind, ParserSettings, PlayerSettings, PosterSize, QualityBand,
    SecretString, Settings, SettingsError, SettingsStore, TorrServerSettings, UiLanguage,
    VideoScale,
};
pub use typograf::typograph;
