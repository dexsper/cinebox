//! Releases from the parser and their files on TorrServer.

use std::sync::Arc;

use cinebox_core::{
    KIND_SEASON, MediaDetails, MediaKind, SEASON_TTL, Store, TmdbId, language_key,
    normalize_tmdb_path, season_cache_id, tmdb_image_url,
};
use cinebox_indexer::{SortMode, TorrentHit, sort_hits};
use cinebox_tmdb::fetch_season_episodes;
use cinebox_torrserver::{file_display_name, parse_file_episode};
use tracing::warn;

use super::{JobError, OpenTarget, ParserCtx, TmdbCtx, TorrCtx};
use crate::screens::torrents::{MovieBits, ReadyFiles, TorrentFileRow};

pub async fn load_torrents(
    parser: ParserCtx,
    torr: TorrCtx,
    details: MediaDetails,
    db: Option<Arc<Store>>,
) -> Result<Vec<TorrentHit>, JobError> {
    let kind = details.kind;
    let query = cinebox_indexer::SearchQuery {
        query: details.torrent_query(),
        title: details.title.clone(),
        original_title: details.original_title.clone().unwrap_or_default(),
        year: details.year,
        kind: details.kind,
        is_anime: details.is_anime(),
        genres: details.genres.clone(),
    };

    let runtime = details.runtime_minutes;
    let raw = cinebox_indexer::search(
        parser.kind,
        &parser.url,
        &parser.api_key,
        &query,
        &parser.net,
    )
    .await?;

    let started = cinebox_torrserver::list(&torr.server)
        .await
        .inspect_err(|error| warn!(%error, "torrserver list failed; started tags unavailable"))
        .unwrap_or_default();

    let hashes: Vec<String> = started.into_iter().map(|row| row.hash).collect();
    let local_hashes = match db.as_ref() {
        Some(db) => match db.watch_release_hashes(kind, details.id).await {
            Ok(hashes) => hashes,
            Err(error) => {
                warn!(%error, "failed to load local torrent hashes");
                Vec::new()
            }
        },
        None => Vec::new(),
    };

    let mut hits: Vec<TorrentHit> = raw
        .into_iter()
        .map(|hit| TorrentHit::new(hit, runtime, &hashes, &local_hashes))
        .collect();

    sort_hits(&mut hits, kind, SortMode::Popular);
    Ok(hits)
}

pub async fn open_magnet(
    torr: TorrCtx,
    tmdb: TmdbCtx,
    spec: cinebox_torrserver::AddSpec,
    movie: MovieBits,
    target: OpenTarget,
    db: Option<Arc<Store>>,
) -> Result<ReadyFiles, JobError> {
    let opened = cinebox_torrserver::open_magnet(&torr.server, &spec, torr.track_timecode).await?;

    let serial = target.kind == MediaKind::Tv || movie.number_of_seasons.is_some();
    let catalog = season_catalog(&opened.files, serial, target.id, &tmdb, db.clone()).await;

    let files = decorate_files(
        opened,
        &movie,
        serial,
        target.runtime_minutes,
        &catalog,
        db.as_deref().map(|db| (db, target.kind, target.id)),
    )
    .await;

    Ok(files)
}

async fn season_catalog(
    files: &[cinebox_torrserver::OpenedFile],
    serial: bool,
    id: TmdbId,
    tmdb: &TmdbCtx,
    db: Option<Arc<Store>>,
) -> Vec<cinebox_tmdb::SeasonEpisode> {
    if !serial || tmdb.api_key.is_empty() {
        return Vec::new();
    }

    let mut seasons: Vec<u32> = files
        .iter()
        .filter_map(|file| parse_file_episode(&file.path, true).season)
        .collect();

    seasons.sort_unstable();
    seasons.dedup();

    if seasons.is_empty() {
        return Vec::new();
    }

    let language = Some(tmdb.language);
    let lang = language_key(language);
    let mut out = Vec::new();
    let mut need = Vec::new();
    for season in seasons {
        let cache_id = season_cache_id(id, season);
        let cached = match db.as_ref() {
            Some(db) => db
                .get_json::<Vec<cinebox_tmdb::SeasonEpisode>>(lang, KIND_SEASON, &cache_id)
                .await
                .ok()
                .flatten(),
            None => None,
        };

        if let Some(hit) = cached {
            if hit.is_fresh(SEASON_TTL) {
                out.extend(hit.value);
                continue;
            }
        }

        need.push(season);
    }

    if need.is_empty() {
        return out;
    }

    let fetched = fetch_season_episodes(&tmdb.api_key, id, &need, language, &tmdb.net)
        .await
        .inspect_err(|error| warn!(%error, "season episode catalog failed"))
        .unwrap_or_default();

    if let Some(db) = db {
        for season in need {
            let eps: Vec<cinebox_tmdb::SeasonEpisode> = fetched
                .iter()
                .filter(|ep| ep.season == season)
                .cloned()
                .collect();

            let paths = episode_paths(&eps);
            let cache_id = season_cache_id(id, season);
            let saved = db
                .put_json(lang, KIND_SEASON, &cache_id, &eps, &paths)
                .await;

            if let Err(error) = saved {
                warn!(%error, "failed to persist season episodes");
            }
        }
    }

    out.extend(fetched);
    out
}

fn episode_paths(episodes: &[cinebox_tmdb::SeasonEpisode]) -> Vec<String> {
    let mut paths = Vec::new();
    for episode in episodes {
        let Some(path) = normalize_tmdb_path(episode.still_path.as_deref()) else {
            continue;
        };

        if paths.contains(&path) {
            continue;
        }

        paths.push(path);
    }

    paths
}

fn file_title(
    named: Option<&cinebox_tmdb::SeasonEpisode>,
    serial: bool,
    human: String,
    movie_title: &str,
) -> String {
    if let Some(ep) = named {
        return ep.name.clone();
    }

    if serial {
        return human;
    }

    movie_title.to_owned()
}

async fn decorate_files(
    opened: cinebox_torrserver::OpenedTorrent,
    movie: &MovieBits,
    serial: bool,
    runtime_minutes: Option<u32>,
    catalog: &[cinebox_tmdb::SeasonEpisode],
    timeline: Option<(&Store, MediaKind, TmdbId)>,
) -> ReadyFiles {
    let fallback_still = tmdb_image_url(movie.backdrop_path.as_deref(), "w300")
        .or_else(|| tmdb_image_url(movie.poster_path.as_deref(), "w300"));

    let mut rows = Vec::new();
    for (index, file) in opened.files.into_iter().enumerate() {
        let parsed = parse_file_episode(&file.path, serial);
        let tmdb = catalog
            .iter()
            .find(|ep| parsed.season == Some(ep.season) && parsed.episode == Some(ep.episode));

        let human = file_display_name(&file.path);
        let named = tmdb.filter(|ep| !ep.name.is_empty());
        let title = file_title(named, serial, human, &movie.title);

        let still_url = tmdb
            .and_then(|ep| tmdb_image_url(ep.still_path.as_deref(), "w300"))
            .or_else(|| fallback_still.clone());

        let runtime = tmdb.and_then(|ep| ep.runtime_minutes).or(runtime_minutes);
        let number = parsed.episode.unwrap_or((index as u32).saturating_add(1));
        let air_date = tmdb
            .and_then(|ep| ep.air_date.as_deref())
            .filter(|d| !d.is_empty())
            .map(crate::i18n::format_release_date);

        let local = match timeline {
            Some((db, kind, id)) => db
                .get_watch_timeline(kind, id, parsed.season, parsed.episode)
                .await
                .ok()
                .flatten(),
            None => None,
        };
        let timecode = local.map(|(time, _)| time).unwrap_or(file.timecode);

        rows.push(TorrentFileRow {
            id: file.id,
            path: file.path,
            length: file.length,
            timecode,
            number,
            season: parsed.season,
            episode: parsed.episode,
            title,
            still_url,
            runtime_minutes: runtime,
            air_date,
        });
    }

    if serial {
        rows.sort_by(|a, b| {
            a.season
                .unwrap_or(0)
                .cmp(&b.season.unwrap_or(0))
                .then(a.episode.unwrap_or(0).cmp(&b.episode.unwrap_or(0)))
                .then(a.path.cmp(&b.path))
        });
    }

    ReadyFiles::from_rows(opened.hash, rows)
}

pub async fn wait_stream(
    torr: TorrCtx,
    file_path: String,
    hash: String,
    file_id: i32,
    resume_bytes: Option<u64>,
    on_event: impl FnMut(cinebox_torrserver::PreloadEvent) + Send,
) -> Result<(), JobError> {
    let target = cinebox_torrserver::PreloadTarget {
        file_path: &file_path,
        hash: &hash,
        index: file_id,
    };

    if let Some(offset) = resume_bytes {
        cinebox_torrserver::wait_preload_at_bytes(&torr.server, target, offset, on_event).await?;
        return Ok(());
    }

    cinebox_torrserver::wait_preload(&torr.server, target, on_event).await?;

    Ok(())
}
