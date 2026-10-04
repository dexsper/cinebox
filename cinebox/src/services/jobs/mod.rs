//! Async jobs spawned on the egui-async Tokio runtime.

mod catalog;
mod health;
mod skip;
mod torrents;
mod youtube;

pub use catalog::{
    cached_section, load_catalog_page, load_discover_page, load_home, load_media, load_person,
    load_search_page, load_section,
};
pub use health::{ping_parser, ping_tmdb, ping_torrserver, speed_test};
pub use skip::{fetch_skip_choices, fetch_skip_segments, save_skip_choice};
pub use torrents::{load_torrents, open_magnet, wait_stream};
pub use youtube::resolve_youtube;

use cinebox_core::{MediaKind, ParserKind, Settings, TmdbId};
use cinebox_net::NetConfig;
use cinebox_torrserver::Server;

/// Job failures, one variant per backing service.
///
/// Rendered as a string only at the UI boundary (toasts, error views).
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error(transparent)]
    Tmdb(#[from] cinebox_tmdb::Error),
    #[error(transparent)]
    Indexer(#[from] cinebox_indexer::Error),
    #[error(transparent)]
    TorrServer(#[from] cinebox_torrserver::Error),
    #[error(transparent)]
    Youtube(#[from] cinebox_youtube::Error),
    #[error(transparent)]
    Skip(#[from] cinebox_skip::Error),
    #[error(transparent)]
    Store(#[from] cinebox_core::StoreError),
}

/// Network snapshot shared by TMDB and parser jobs (the setting is global).
pub fn net_config(settings: &Settings) -> NetConfig {
    NetConfig {
        use_system_proxy: settings.general.use_system_proxy,
        dns_bypass: settings.general.dns_bypass,
        custom_doh_url: usable_doh_url(&settings.general.custom_doh_url),
    }
}

/// A half-typed or `http://` DoH endpoint would only add a failing hop; drop it.
fn usable_doh_url(url: &str) -> String {
    if crate::services::settings_input::doh_url_ok(url) {
        return url.to_owned();
    }

    String::new()
}

/// Narrow snapshot of the TMDB settings a job needs.
#[derive(Clone)]
pub struct TmdbCtx {
    pub api_key: String,
    pub language: &'static str,
    pub net: NetConfig,
}

impl From<&Settings> for TmdbCtx {
    fn from(settings: &Settings) -> Self {
        Self {
            api_key: settings.tmdb.api_key.expose().to_owned(),
            language: settings.general.language.tmdb_code(),
            net: net_config(settings),
        }
    }
}

/// Narrow snapshot of the parser (Jackett / Prowlarr) settings a job needs.
#[derive(Clone)]
pub struct ParserCtx {
    pub kind: ParserKind,
    pub url: String,
    pub api_key: String,
    pub net: NetConfig,
}

impl From<&Settings> for ParserCtx {
    fn from(settings: &Settings) -> Self {
        Self {
            kind: settings.parser.kind,
            url: settings.parser.url.clone(),
            api_key: settings.parser.api_key.expose().to_owned(),
            net: net_config(settings),
        }
    }
}

/// Narrow snapshot of the TorrServer settings a job needs.
#[derive(Clone)]
pub struct TorrCtx {
    pub server: Server,
    pub track_timecode: bool,
}

impl From<&Settings> for TorrCtx {
    fn from(settings: &Settings) -> Self {
        let torrserver = &settings.torrserver;
        let server = Server {
            url: torrserver.url.clone(),
            username: torrserver.username.clone(),
            password: torrserver.password.expose().to_owned(),
        };

        Self {
            server,
            track_timecode: torrserver.track_timecode,
        }
    }
}

/// The media an opened torrent belongs to.
#[derive(Clone, Copy)]
pub struct OpenTarget {
    pub kind: MediaKind,
    pub id: TmdbId,
    pub runtime_minutes: Option<u32>,
}
