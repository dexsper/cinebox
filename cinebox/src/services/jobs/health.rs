//! Whether TMDB, the parser and TorrServer answer, and how fast.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use cinebox_core::{CONFIG_TTL, KIND_CONFIG, Store};
use tracing::warn;

use super::{JobError, ParserCtx, TmdbCtx, TorrCtx};

pub async fn ping_torrserver(torr: TorrCtx) -> Result<String, JobError> {
    let echo = cinebox_torrserver::echo(&torr.server).await?;

    Ok(echo)
}

pub async fn ping_parser(parser: ParserCtx) -> Result<String, JobError> {
    let version =
        cinebox_indexer::ping(parser.kind, &parser.url, &parser.api_key, &parser.net).await?;

    Ok(version)
}

pub async fn ping_tmdb(tmdb: TmdbCtx, db: Option<Arc<Store>>) -> Result<String, JobError> {
    let fp = key_fingerprint(&tmdb.api_key);
    let cache_id = format!("ping:{fp:x}");
    if let Some(db) = &db {
        let cached = db.get_json::<String>("", KIND_CONFIG, &cache_id).await;

        if let Ok(Some(hit)) = cached
            && hit.is_fresh(CONFIG_TTL)
        {
            return Ok(hit.value);
        }
    }

    let result = cinebox_tmdb::check_api_key(&tmdb.api_key, &tmdb.net).await;

    if let Ok(msg) = &result
        && let Some(db) = db
    {
        let saved = db.put_json("", KIND_CONFIG, &cache_id, msg, &[]).await;

        if let Err(error) = saved {
            warn!(%error, "failed to persist tmdb ping");
        }
    }

    Ok(result?)
}

fn key_fingerprint(key: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

pub async fn speed_test(
    torr: TorrCtx,
    on_event: impl FnMut(cinebox_torrserver::SpeedEvent) + Send,
) -> Result<f64, JobError> {
    let report = cinebox_torrserver::speed_test(&torr.server, on_event).await?;
    Ok(report.megabits_per_sec())
}
