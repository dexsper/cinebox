//! Async jobs spawned on the egui-async Tokio runtime.

mod catalog;
mod health;
mod skip;
mod torrents;
mod torrserver_sync;
mod youtube;

pub(crate) use catalog::{
    cached_section, load_catalog_page, load_discover_page, load_home, load_media, load_person, load_search_page,
    load_section,
};
pub(crate) use health::{ping_parser, ping_tmdb, ping_torrserver, probe_tmdb_proxy, speed_test};
pub(crate) use skip::{fetch_skip_choices, fetch_skip_segments, save_skip_choice, skip_cache_id};
pub(crate) use torrents::{OpenMagnet, StreamFile, load_torrents, open_magnet, wait_stream};
pub(crate) use torrserver_sync::{SYNC_INTERVAL, SyncOutcome, sync_from_server};
pub(crate) use youtube::resolve_youtube;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cinebox_core::{MediaKind, ParserKind, Settings, TmdbId};
use cinebox_net::NetConfig;
use cinebox_torrserver::Server;

/// Job failures, one variant per backing service.
///
/// Rendered as a string only at the UI boundary (toasts, error views).
#[derive(Debug, thiserror::Error)]
pub(crate) enum JobError {
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
pub(crate) fn net_config(settings: &Settings) -> NetConfig {
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

/// Raised by a TMDB job when the proxy refused a request sent without a key, taken by the app once a frame.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeylessRefusal(Arc<AtomicBool>);

impl KeylessRefusal {
    // Relaxed: the flag guards no other data, and a refusal seen a frame late changes nothing.
    fn raise(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether a refusal came in since the last call.
    pub(crate) fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

/// Narrow snapshot of the TMDB settings a job needs.
#[derive(Clone)]
pub(crate) struct TmdbCtx {
    /// `Err` is why the key cannot be sent; a job reports it once it needs TMDB.
    pub api: Result<cinebox_tmdb::Api, cinebox_tmdb::KeyError>,

    pub language: &'static str,

    pub net: NetConfig,

    /// Set while requests go without a key, so that a 401 can tell the app the proxy wants one now.
    refusal: Option<KeylessRefusal>,
}

impl TmdbCtx {
    pub(crate) fn new(settings: &Settings, refusal: &KeylessRefusal) -> Self {
        let tmdb = &settings.tmdb;
        let key = (!tmdb.keyless).then(|| tmdb.api_key.expose());
        let base = cinebox_tmdb::api_base(&tmdb.proxy_url);

        Self {
            api: cinebox_tmdb::Api::new(&base, key),
            language: settings.general.language.tmdb_code(),
            net: net_config(settings),
            refusal: tmdb.keyless.then(|| refusal.clone()),
        }
    }

    pub(crate) fn api(&self) -> Result<&cinebox_tmdb::Api, cinebox_tmdb::Error> {
        self.api.as_ref().map_err(|error| cinebox_tmdb::Error::from(*error))
    }

    /// Passes `result` through; a keyless request the server refused raises the refusal.
    pub(crate) fn note<T>(&self, result: Result<T, cinebox_tmdb::Error>) -> Result<T, cinebox_tmdb::Error> {
        let is_refused = matches!(result, Err(cinebox_tmdb::Error::Unauthorized));
        if is_refused && let Some(refusal) = &self.refusal {
            refusal.raise();
        }

        result
    }

    /// Like [`Self::note`], for shelves that keep their failure as text.
    pub(crate) fn note_shelf_errors<'a>(&self, errors: impl IntoIterator<Item = Option<&'a str>>) {
        let Some(refusal) = &self.refusal else { return };

        let refused = cinebox_tmdb::Error::Unauthorized.to_string();
        let mut texts = errors.into_iter().flatten();
        if texts.any(|text| text == refused) {
            refusal.raise();
        }
    }
}

/// Narrow snapshot of the parser (Jackett / Prowlarr) settings a job needs.
#[derive(Clone)]
pub(crate) struct ParserCtx {
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
pub(crate) struct TorrCtx {
    pub server: Server,

    /// Positions are read from `/viewed` (see `TorrServerSettings::is_tracking_position`).
    pub track_timecode: bool,

    /// Torrents and positions other clients left on the server are taken in.
    pub sync_history: bool,
}

impl From<&Settings> for TorrCtx {
    fn from(settings: &Settings) -> Self {
        let torrserver = &settings.torrserver;
        let server = Server {
            url: torrserver.url.clone(),
            username: torrserver.username.clone(),
            password: torrserver.password.expose().to_owned(),
        };

        Self { server, track_timecode: torrserver.is_tracking_position(), sync_history: torrserver.sync_history }
    }
}

/// The media an opened torrent belongs to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OpenTarget {
    pub kind: MediaKind,
    pub id: TmdbId,
    pub runtime_minutes: Option<u32>,
}
