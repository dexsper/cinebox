//! Releases from the parser and their files on TorrServer.

use std::sync::Arc;

use cinebox_core::{
    CacheKey, KIND_SEASON, MediaDetails, MediaKind, SEASON_TTL, Store, TmdbId, WatchKey, language_key,
    normalize_tmdb_path, season_cache_id, tmdb_image_url,
};
use cinebox_indexer::{Hit, SortMode, TorrentHit, infohash, sort_hits};
use cinebox_tmdb::fetch_season_episodes;
use cinebox_torrserver::{
    ListedTorrent, OpenedFile, decode_data, file_display_name, is_foreign_timecode, merge_timecode, parse_file_episode,
};
use tracing::warn;

use super::{JobError, OpenTarget, ParserCtx, TmdbCtx, TorrCtx};
use crate::screens::torrents::{MovieBits, ReadyFiles, TorrentFileRow};

const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

pub(crate) async fn load_torrents(
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
    let raw = cinebox_indexer::search(parser.kind, &parser.url, &parser.api_key, &query, &parser.net).await?;

    let started = cinebox_torrserver::list(&torr.server)
        .await
        .inspect_err(|error| warn!(%error, "torrserver list failed; started tags unavailable"))
        .unwrap_or_default();

    let hashes: Vec<String> = started.iter().map(|row| row.hash.clone()).collect();
    let local_hashes = match db.as_ref() {
        Some(db) => match db.watch_release_hashes(kind, details.id).await {
            Ok(local) => local,
            Err(error) => {
                warn!(%error, "failed to load local torrent hashes");
                Vec::new()
            }
        },
        None => Vec::new(),
    };

    let ranked = raw.into_iter().map(|hit| TorrentHit::new(hit, runtime, &hashes, &local_hashes));
    let mut hits: Vec<TorrentHit> = ranked.collect();

    if torr.sync_history {
        let title = (kind, details.id);
        let linked = linked_hashes(db.as_deref(), kind, details.id).await;
        let server_only = started.iter().filter(|torrent| is_server_only(torrent, title, &linked, &hits));

        // Collected first: the filter reads `hits`, which the loop grows.
        let extra: Vec<Hit> = server_only.map(server_hit).collect();

        for hit in extra {
            hits.push(TorrentHit::new(hit, runtime, &hashes, &local_hashes));
        }
    }

    sort_hits(&mut hits, kind, SortMode::Popular);
    Ok(hits)
}

/// Server torrents the sync found to be this title.
async fn linked_hashes(db: Option<&Store>, kind: MediaKind, id: TmdbId) -> Vec<String> {
    let Some(db) = db else { return Vec::new() };

    db.torrserver_hashes(kind, id)
        .await
        .inspect_err(|error| warn!(%error, "failed to load torrserver matches"))
        .unwrap_or_default()
}

/// A torrent of this title on the server that the parser did not return, such as one added from a
/// private tracker on another device.
fn is_server_only(torrent: &ListedTorrent, title: (MediaKind, TmdbId), linked: &[String], hits: &[TorrentHit]) -> bool {
    let card_title = decode_data(&torrent.data).and_then(|card| card.tmdb_ref());
    let is_linked = linked.iter().any(|hash| hash.eq_ignore_ascii_case(&torrent.hash));
    if card_title != Some(title) && !is_linked {
        return false;
    }

    let is_same_hash = |hash: String| hash.eq_ignore_ascii_case(&torrent.hash);
    let is_listed = |hit: &TorrentHit| infohash(&hit.magnet).is_some_and(is_same_hash);

    !hits.iter().any(is_listed)
}

fn server_hit(torrent: &ListedTorrent) -> Hit {
    let name = magnet_name(&torrent.title);

    Hit {
        title: torrent.title.clone(),
        tracker: String::from("TorrServer"),
        size_bytes: torrent.size_bytes,
        seeders: 0,
        peers: 0,
        magnet: format!("magnet:?xt=urn:btih:{}&dn={name}", torrent.hash),
        published: String::new(),
    }
}

/// `title` percent-encoded for a magnet's `dn`.
fn magnet_name(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for byte in title.bytes() {
        let is_plain = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if is_plain {
            out.push(char::from(byte));
            continue;
        }

        let digits = [HEX_DIGITS[usize::from(byte >> 4)], HEX_DIGITS[usize::from(byte & 0x0F)]];
        out.push('%');
        out.extend(digits.map(char::from));
    }

    out
}

/// A magnet to add to TorrServer and the title its files belong to.
#[derive(Debug)]
pub(crate) struct OpenMagnet {
    pub spec: cinebox_torrserver::AddSpec,
    pub movie: MovieBits,
    pub target: OpenTarget,
}

/// A torrent file to wait for until TorrServer has preloaded it.
#[derive(Debug)]
pub(crate) struct StreamFile {
    pub path: String,
    pub hash: String,
    pub id: i32,
    /// Preload from this byte offset instead of the start.
    pub resume_bytes: Option<u64>,
}

struct EpisodeHints<'a> {
    is_serial: bool,
    runtime_minutes: Option<u32>,
    catalog: &'a [cinebox_tmdb::SeasonEpisode],
}

/// Where the positions of an opened torrent live on this device.
#[derive(Clone, Copy)]
struct Timeline<'a> {
    db: &'a Store,

    kind: MediaKind,

    id: TmdbId,

    /// A position another client moved on the server wins over the local one (see `merge_timecode`).
    is_synced: bool,
}

pub(crate) async fn open_magnet(
    torr: TorrCtx,
    tmdb: TmdbCtx,
    request: OpenMagnet,
    db: Option<Arc<Store>>,
) -> Result<ReadyFiles, JobError> {
    let OpenMagnet { spec, movie, target } = request;
    let opened = cinebox_torrserver::open_magnet(&torr.server, &spec, torr.track_timecode).await?;

    let is_serial = target.kind == MediaKind::Tv || movie.number_of_seasons.is_some();
    let catalog = season_catalog(&opened.files, is_serial, target.id, &tmdb, db.clone()).await;
    let hints = EpisodeHints { is_serial, runtime_minutes: target.runtime_minutes, catalog: &catalog };
    let is_synced = torr.sync_history;
    let timeline = db.as_deref().map(|db| Timeline { db, kind: target.kind, id: target.id, is_synced });

    Ok(decorate_files(opened, &movie, &hints, timeline).await)
}

async fn season_catalog(
    files: &[OpenedFile],
    serial: bool,
    id: TmdbId,
    tmdb: &TmdbCtx,
    db: Option<Arc<Store>>,
) -> Vec<cinebox_tmdb::SeasonEpisode> {
    if !serial || tmdb.api_key.is_empty() {
        return Vec::new();
    }

    let mut seasons: Vec<u32> = files.iter().filter_map(|file| parse_file_episode(&file.path, true).season).collect();

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
                .get_json::<Vec<cinebox_tmdb::SeasonEpisode>>(CacheKey {
                    language: lang,
                    kind: KIND_SEASON,
                    id: &cache_id,
                })
                .await
                .ok()
                .flatten(),
            None => None,
        };

        // An empty list is never written now, but older builds cached one after a failed fetch.
        let usable = cached.filter(|hit| hit.is_fresh(SEASON_TTL) && !hit.value.is_empty());
        if let Some(hit) = usable {
            out.extend(hit.value);
            continue;
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
            let in_season = fetched.iter().filter(|episode| episode.season == season);
            let eps: Vec<cinebox_tmdb::SeasonEpisode> = in_season.cloned().collect();

            // Nothing back can be a network failure as well as a missing season,
            // so it is asked again next time instead of hiding names for a day.
            if eps.is_empty() {
                continue;
            }

            let paths = episode_paths(&eps);
            let cache_id = season_cache_id(id, season);
            let saved = db.put_json(CacheKey { language: lang, kind: KIND_SEASON, id: &cache_id }, &eps, &paths).await;

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

fn file_title(named: Option<&cinebox_tmdb::SeasonEpisode>, serial: bool, human: String, movie_title: &str) -> String {
    if let Some(ep) = named {
        return ep.name.clone();
    }

    if serial {
        return human;
    }

    movie_title.to_owned()
}

/// The `/viewed` positions of the torrent this device saw last, when the timeline is synced.
async fn seen_positions(timeline: Option<Timeline<'_>>, hash: &str) -> Vec<(i32, f64)> {
    let Some(timeline) = timeline.filter(|timeline| timeline.is_synced) else { return Vec::new() };

    timeline
        .db
        .viewed_seen(hash)
        .await
        .inspect_err(|error| warn!(%error, "failed to load seen positions"))
        .unwrap_or_default()
}

/// The position to resume `file` at: the local one, unless the timeline is synced and another client moved
/// the server's since this device last saw it. Such a position is taken in locally.
async fn resume_timecode(
    timeline: Timeline<'_>,
    key: WatchKey,
    hash: &str,
    file: &OpenedFile,
    seen: Option<f64>,
) -> f64 {
    let local = timeline.db.get_watch_timeline(key).await.ok().flatten().map(|(time, _)| time);
    if !timeline.is_synced {
        return local.unwrap_or(file.timecode);
    }

    let merged = merge_timecode(local, file.timecode, seen);
    if !is_foreign_timecode(file.timecode, seen) {
        return merged;
    }

    // Kept locally and marked seen, or the next open would find the old local position newer again.
    if let Err(error) = timeline.db.import_watch_timeline(key, merged).await {
        warn!(%error, "failed to take in the server position");
    }

    if let Err(error) = timeline.db.put_viewed_seen(hash, file.id, file.timecode).await {
        warn!(%error, "failed to record the seen server position");
    }

    merged
}

async fn decorate_files(
    opened: cinebox_torrserver::OpenedTorrent,
    movie: &MovieBits,
    hints: &EpisodeHints<'_>,
    timeline: Option<Timeline<'_>>,
) -> ReadyFiles {
    let EpisodeHints { is_serial, runtime_minutes, catalog } = *hints;

    let backdrop_still = tmdb_image_url(movie.backdrop_path.as_deref(), "w300");
    let fallback_still = backdrop_still.or_else(|| tmdb_image_url(movie.poster_path.as_deref(), "w300"));

    let mut rows = Vec::new();
    let seen = seen_positions(timeline, &opened.hash).await;

    for (index, file) in opened.files.into_iter().enumerate() {
        let parsed = parse_file_episode(&file.path, is_serial);
        let is_parsed = |episode: &&cinebox_tmdb::SeasonEpisode| {
            parsed.season == Some(episode.season) && parsed.episode == Some(episode.episode)
        };
        let tmdb = catalog.iter().find(is_parsed);

        let human = file_display_name(&file.path);
        let named = tmdb.filter(|episode| !episode.name.is_empty());
        let title = file_title(named, is_serial, human, &movie.title);

        let tmdb_still = tmdb.and_then(|episode| tmdb_image_url(episode.still_path.as_deref(), "w300"));
        let still_url = tmdb_still.or_else(|| fallback_still.clone());

        let runtime = tmdb.and_then(|episode| episode.runtime_minutes).or(runtime_minutes);
        let position = u32::try_from(index).unwrap_or(u32::MAX);
        let number = parsed.episode.unwrap_or(position.saturating_add(1));
        let tmdb_air_date = tmdb.and_then(|episode| episode.air_date.as_deref()).filter(|date| !date.is_empty());
        let air_date = tmdb_air_date.map(crate::i18n::format_release_date);

        let timecode = match timeline {
            Some(timeline) => {
                let Timeline { kind, id, .. } = timeline;
                let key = WatchKey { kind, id, season: parsed.season, episode: parsed.episode };
                let seen_here = seen.iter().find(|(seen_id, _)| *seen_id == file.id).map(|(_, timecode)| *timecode);
                resume_timecode(timeline, key, &opened.hash, &file, seen_here).await
            }
            None => file.timecode,
        };

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

    if is_serial {
        rows.sort_by(|left, right| {
            let left_key = (left.season.unwrap_or(0), left.episode.unwrap_or(0), &left.path);
            let right_key = (right.season.unwrap_or(0), right.episode.unwrap_or(0), &right.path);

            left_key.cmp(&right_key)
        });
    }

    ReadyFiles::from_rows(opened.hash, rows)
}

pub(crate) async fn wait_stream(
    torr: TorrCtx,
    file: StreamFile,
    on_event: impl FnMut(cinebox_torrserver::PreloadEvent) + Send,
) -> Result<(), JobError> {
    let target = cinebox_torrserver::PreloadTarget { file_path: &file.path, hash: &file.hash, index: file.id };

    if let Some(offset) = file.resume_bytes {
        cinebox_torrserver::wait_preload_at_bytes(&torr.server, target, offset, on_event).await?;
        return Ok(());
    }

    cinebox_torrserver::wait_preload(&torr.server, target, on_event).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use cinebox_core::{Section, StoreError};
    use cinebox_torrserver::OpenedTorrent;

    use super::*;
    use crate::services::db_block_on;

    const MOVIE: WatchKey = WatchKey { kind: MediaKind::Movie, id: TmdbId::new(7), season: None, episode: None };

    fn movie() -> MovieBits {
        MovieBits {
            title: String::from("Movie"),
            overview: None,
            year: None,
            vote: None,
            certification: None,
            poster_path: None,
            backdrop_path: None,
            number_of_seasons: None,
            section: Section::Movies,
            head_line: String::new(),
            genres_line: String::new(),
        }
    }

    /// A one-file movie whose server position is `server`, with a local one of 600 s.
    fn opened(db: &Store, server: f64) -> Result<OpenedTorrent, StoreError> {
        db_block_on(db.upsert_watch_timeline(MOVIE, 600.0, 7200.0))?;

        let file = OpenedFile { id: 1, path: String::from("movie.mkv"), length: 1, timecode: server };

        Ok(OpenedTorrent { hash: String::from("aa"), files: vec![file], resume_id: Some(1) })
    }

    fn decorated(db: &Store, opened: OpenedTorrent, is_synced: bool) -> f64 {
        let hints = EpisodeHints { is_serial: false, runtime_minutes: None, catalog: &[] };
        let timeline = Timeline { db, kind: MOVIE.kind, id: MOVIE.id, is_synced };
        let ready = db_block_on(decorate_files(opened, &movie(), &hints, Some(timeline)));

        ready.files[0].timecode
    }

    #[test]
    fn without_sync_the_local_position_wins() -> Result<(), StoreError> {
        let db = db_block_on(Store::memory())?;
        let opened = opened(&db, 1200.0)?;

        let timecode = decorated(&db, opened, false);

        assert!((timecode - 600.0).abs() < f64::EPSILON);
        assert_eq!(db_block_on(db.viewed_seen("aa"))?, Vec::new());

        Ok(())
    }

    #[test]
    fn with_sync_a_position_moved_elsewhere_wins_and_is_kept() -> Result<(), StoreError> {
        let db = db_block_on(Store::memory())?;
        let opened = opened(&db, 1200.0)?;
        db_block_on(db.put_viewed_seen("aa", 1, 300.0))?;

        let timecode = decorated(&db, opened, true);

        assert!((timecode - 1200.0).abs() < f64::EPSILON);
        assert_eq!(db_block_on(db.get_watch_timeline(MOVIE))?, Some((1200.0, 7200.0)));
        assert_eq!(db_block_on(db.viewed_seen("aa"))?, vec![(1, 1200.0)]);

        Ok(())
    }

    #[test]
    fn with_sync_the_local_position_wins_over_what_this_device_sent() -> Result<(), StoreError> {
        let db = db_block_on(Store::memory())?;
        let opened = opened(&db, 300.0)?;
        db_block_on(db.put_viewed_seen("aa", 1, 300.0))?;

        let timecode = decorated(&db, opened, true);

        assert!((timecode - 600.0).abs() < f64::EPSILON);

        Ok(())
    }

    #[test]
    fn a_server_torrent_of_this_title_is_listed_once() {
        let torrent = ListedTorrent {
            hash: "AB12".repeat(10),
            title: String::from("Дюна / Dune"),
            poster: String::new(),
            category: String::from("movie"),
            data: String::from(r#"{"movie":{"id":7}}"#),
            timestamp: 0,
            size_bytes: 10,
            file_stats: Vec::new(),
        };
        let title = (MOVIE.kind, MOVIE.id);

        let hit = server_hit(&torrent);
        let listed = TorrentHit::new(hit.clone(), None, &[], &[]);

        assert!(is_server_only(&torrent, title, &[], &[]));
        assert!(!is_server_only(&torrent, title, &[], &[listed]));
        assert!(!is_server_only(&torrent, (MediaKind::Movie, TmdbId::new(8)), &[], &[]));
        assert_eq!(
            hit.magnet,
            format!("magnet:?xt=urn:btih:{}&dn=%D0%94%D1%8E%D0%BD%D0%B0%20%2F%20Dune", torrent.hash)
        );
    }
}
