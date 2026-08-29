//! Shared models, settings persistence, and i18n keys.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod http;
pub mod i18n;
pub mod ids;
pub mod media;
pub mod settings;
pub mod typograf;

pub use catalog::{CatalogItem, HomeCatalog, HomeRow, HomeRowId, tmdb_image_url, year_from_date};
pub use http::{BaseUrlError, join_url, normalize_base_url};
pub use ids::{MediaKind, TmdbId};
pub use media::{
    CreditPerson, MediaDetails, PersonDetails, Trailer, format_money, format_release_date,
    format_runtime,
};
pub use settings::{
    DefaultQuality, ParserKind, PlayerSettings, PosterSize, SecretString, Settings, SettingsError,
    SettingsStore, TorrServerSettings, UiLanguage, VideoScale,
};
pub use typograf::typograph;
