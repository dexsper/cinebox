//! Whether TMDB, the parser and TorrServer answer, and how fast.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use cinebox_core::{CONFIG_TTL, CacheKey, KIND_CONFIG, Store};
use cinebox_net::NetConfig;
use tracing::warn;

use super::{JobError, ParserCtx, TmdbCtx, TorrCtx};

pub(crate) async fn ping_torrserver(torr: TorrCtx) -> Result<String, JobError> {
    let echo = cinebox_torrserver::echo(&torr.server).await?;

    Ok(echo)
}

pub(crate) async fn ping_parser(parser: ParserCtx) -> Result<String, JobError> {
    let version = cinebox_indexer::ping(parser.kind, &parser.url, &parser.api_key, &parser.net).await?;

    Ok(version)
}

pub(crate) async fn ping_tmdb(tmdb: TmdbCtx, db: Option<Arc<Store>>) -> Result<String, JobError> {
    let api = tmdb.api()?;
    let fp = api_fingerprint(api);
    let cache_id = format!("ping:{fp:x}");
    if let Some(db) = &db {
        let cached = db.get_json::<String>(CacheKey { language: "", kind: KIND_CONFIG, id: &cache_id }).await;

        if let Ok(Some(hit)) = cached
            && hit.is_fresh(CONFIG_TTL)
        {
            return Ok(hit.value);
        }
    }

    let checked = cinebox_tmdb::check_api_key(api, &tmdb.net).await;
    let result = tmdb.note(checked);

    if let Ok(msg) = &result
        && let Some(db) = db
    {
        let saved = db.put_json(CacheKey { language: "", kind: KIND_CONFIG, id: &cache_id }, msg, &[]).await;

        if let Err(error) = saved {
            warn!(%error, "failed to persist tmdb ping");
        }
    }

    Ok(result?)
}

/// Covers the base too, so that a key checked against one server is checked again against another.
fn api_fingerprint(api: &cinebox_tmdb::Api) -> u64 {
    let mut hasher = DefaultHasher::new();
    api.hash(&mut hasher);
    hasher.finish()
}

/// Whether the server at `proxy_url` is a `cinebox-tmdb-proxy`, and if so whether it needs the viewer's key.
///
/// `None` without a request for an address the catalog refuses as insecure: an answer over plain http could be
/// forged to switch keyless mode on.
pub(crate) async fn probe_tmdb_proxy(proxy_url: String, net: NetConfig) -> Option<cinebox_tmdb::ProxyInfo> {
    if !crate::services::settings_input::tmdb_url_ok(&proxy_url) {
        return None;
    }

    cinebox_tmdb::probe_proxy(&proxy_url, &net).await
}

pub(crate) async fn speed_test(
    torr: TorrCtx,
    on_event: impl FnMut(cinebox_torrserver::SpeedEvent) + Send,
) -> Result<f64, JobError> {
    let report = cinebox_torrserver::speed_test(&torr.server, on_event).await?;
    Ok(report.megabits_per_sec())
}
