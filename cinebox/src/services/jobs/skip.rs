//! Intro and credits segments to skip.

use std::collections::HashMap;
use std::sync::Arc;

use cinebox_core::{CacheKey, KIND_SKIP, MediaKind, SKIP_SEGMENTS_TTL, Store, TmdbId};
use cinebox_net::NetConfig;
use cinebox_skip::SegmentType;
use tracing::warn;

use super::JobError;

pub(crate) fn skip_cache_id(
    tmdb_id: TmdbId,
    kind: MediaKind,
    season: Option<u32>,
    episode: Option<u32>,
    duration_ms: u64,
) -> String {
    let kind_str = cinebox_core::media_kind_key(kind);
    let tmdb_id = tmdb_id.get();

    match (season, episode) {
        (Some(season), Some(episode)) => format!("{kind_str}:{tmdb_id}:{season}:{episode}:{duration_ms}"),
        _ => format!("{kind_str}:{tmdb_id}:{duration_ms}"),
    }
}

pub(crate) async fn fetch_skip_segments(
    net: NetConfig,
    db: Option<Arc<Store>>,
    query: cinebox_skip::SegmentQuery,
    cache_id: String,
) -> Result<Option<cinebox_skip::MediaSegments>, JobError> {
    let key = CacheKey { language: "", kind: KIND_SKIP, id: &cache_id };

    if let Some(ref db) = db {
        let cached = db.get_json::<cinebox_skip::MediaSegments>(key).await;

        if let Ok(Some(hit)) = cached {
            if hit.is_fresh(SKIP_SEGMENTS_TTL) {
                return Ok(Some(hit.value));
            }
        }
    }

    let provider = cinebox_skip::providers::introdb::IntroDbProvider::default();
    let result = cinebox_skip::SegmentProvider::fetch(&provider, &query, &net).await?;

    if let Some(ref db) = db {
        if let Some(ref segments) = result {
            if let Err(error) = db.put_json(key, segments, &[]).await {
                warn!(%error, "failed to cache skip segments");
            }
        }
    }

    Ok(result)
}

pub(crate) async fn save_skip_choice(
    db: Arc<Store>,
    kind: MediaKind,
    tmdb_id: TmdbId,
    segment_type: SegmentType,
    armed: bool,
) -> Result<(), JobError> {
    let kind_str = cinebox_core::media_kind_key(kind);
    let id = i64::from(tmdb_id.get());

    db.set_skip_armed(kind_str, id, segment_type.as_str(), armed).await?;

    Ok(())
}

pub(crate) async fn fetch_skip_choices(
    db: Arc<Store>,
    kind: MediaKind,
    tmdb_id: TmdbId,
) -> Result<HashMap<SegmentType, bool>, JobError> {
    let id = i64::from(tmdb_id.get());
    let kind_str = cinebox_core::media_kind_key(kind);
    let raw = db.get_skip_choices(kind_str, id).await?;

    let mut out = HashMap::new();
    for ty in SegmentType::ALL {
        let armed = raw.get(ty.as_str()).copied().unwrap_or(false);
        out.insert(ty, armed);
    }

    Ok(out)
}
