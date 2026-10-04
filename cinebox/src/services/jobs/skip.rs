//! Intro and credits segments to skip.

use std::collections::HashMap;
use std::sync::Arc;

use cinebox_core::{KIND_SKIP, MediaKind, SKIP_SEGMENTS_TTL, Store, TmdbId};
use cinebox_net::NetConfig;
use cinebox_skip::SegmentType;

use super::JobError;

pub fn skip_cache_id(
    tmdb_id: TmdbId,
    kind: MediaKind,
    season: Option<u32>,
    episode: Option<u32>,
    duration_ms: u64,
) -> String {
    let kind_str = cinebox_core::media_kind_key(kind);
    match (season, episode) {
        (Some(s), Some(e)) => format!(
            "{kind_str}:{tmdb_id}:{s}:{e}:{duration_ms}",
            tmdb_id = tmdb_id.get()
        ),
        _ => format!(
            "{kind_str}:{tmdb_id}:{duration_ms}",
            tmdb_id = tmdb_id.get()
        ),
    }
}

pub async fn fetch_skip_segments(
    net: NetConfig,
    db: Option<Arc<Store>>,
    query: cinebox_skip::SegmentQuery,
) -> Result<Option<cinebox_skip::MediaSegments>, JobError> {
    let cache_id = skip_cache_id(
        TmdbId::new(query.tmdb_id as u32),
        query.kind,
        query.season,
        query.episode,
        query.duration_ms,
    );

    if let Some(ref db) = db {
        let cached = db
            .get_json::<cinebox_skip::MediaSegments>("", KIND_SKIP, &cache_id)
            .await;

        if let Ok(Some(hit)) = cached {
            if hit.is_fresh(SKIP_SEGMENTS_TTL) {
                return Ok(Some(hit.value));
            }
        }
    }

    let provider = cinebox_skip::providers::introdb::IntroDbProvider;
    let result = cinebox_skip::SegmentProvider::fetch(&provider, &query, &net).await?;

    if let Some(ref db) = db {
        if let Some(ref segments) = result {
            let _ = db.put_json("", KIND_SKIP, &cache_id, segments, &[]).await;
        }
    }

    Ok(result)
}

pub async fn save_skip_choice(
    db: Arc<Store>,
    kind: MediaKind,
    tmdb_id: TmdbId,
    segment_type: SegmentType,
    armed: bool,
) -> Result<(), JobError> {
    let kind_str = cinebox_core::media_kind_key(kind);
    let id = i64::from(tmdb_id.get());

    db.set_skip_armed(kind_str, id, segment_type.as_str(), armed)
        .await?;

    Ok(())
}

pub async fn fetch_skip_choices(
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
