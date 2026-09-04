//! Async jobs spawned on the egui-async Tokio runtime.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use cinebox_core::{
    CONFIG_TTL, HomeCatalog, HomeRow, HomeRowId, KIND_CONFIG, KIND_HOME, KIND_MEDIA, KIND_PERSON,
    KIND_SEASON, MediaDetails, MediaKind, ParserKind, PersonDetails, SEASON_TTL, Settings, Store,
    TmdbId, format_release_date, language_key, media_cache_id, normalize_tmdb_path,
    person_cache_id, season_cache_id, tmdb_image_url,
};
use cinebox_net::NetConfig;
use cinebox_parse::{
    Listing, SortMode, TorrentHit, file_display_name, parse_file_episode, sort_hits,
};
use tracing::warn;

use crate::screens::torrents::{MovieBits, ReadyFiles, TorrentFileRow};

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
}

/// Network snapshot shared by TMDB and parser jobs (the setting is global).
pub fn net_config(settings: &Settings) -> NetConfig {
    NetConfig {
        use_system_proxy: settings.general.use_system_proxy,
        dns_bypass: settings.general.dns_bypass,
        custom_doh_url: settings.general.custom_doh_url.clone(),
    }
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
    pub url: String,
    pub username: String,
    pub password: String,
    pub track_timecode: bool,
}

impl From<&Settings> for TorrCtx {
    fn from(settings: &Settings) -> Self {
        Self {
            url: settings.torrserver.url.clone(),
            username: settings.torrserver.username.clone(),
            password: settings.torrserver.password.expose().to_owned(),
            track_timecode: settings.torrserver.track_timecode,
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

fn listing_from_hit(hit: cinebox_indexer::Hit) -> Listing {
    Listing {
        title: hit.title,
        tracker: hit.tracker,
        size_bytes: hit.size_bytes,
        seeders: hit.seeders,
        peers: hit.peers,
        magnet: hit.magnet,
        published: hit.published,
    }
}

pub async fn load_catalog_page(
    tmdb: TmdbCtx,
    id: HomeRowId,
    page: u32,
) -> Result<cinebox_tmdb::CatalogPage, JobError> {
    let page =
        cinebox_tmdb::fetch_catalog_page(&tmdb.api_key, id, page, Some(tmdb.language), &tmdb.net)
            .await?;

    Ok(page)
}

pub async fn load_search_page(
    tmdb: TmdbCtx,
    query: String,
    kind: cinebox_tmdb::SearchKind,
    page: u32,
) -> Result<cinebox_tmdb::CatalogPage, JobError> {
    let page = cinebox_tmdb::fetch_search_page(
        &tmdb.api_key,
        &query,
        kind,
        page,
        Some(tmdb.language),
        &tmdb.net,
    )
    .await?;

    Ok(page)
}

pub async fn load_home(tmdb: TmdbCtx, db: Option<Arc<Store>>) -> Result<HomeCatalog, JobError> {
    let language = Some(tmdb.language);
    let fetched = cinebox_tmdb::fetch_home(&tmdb.api_key, language, &tmdb.net).await?;

    let Some(db) = db else {
        return Ok(fetched);
    };

    let lang = language_key(Some(tmdb.language));
    let mut rows = Vec::with_capacity(fetched.rows.len());
    for row in fetched.rows {
        if row.id == HomeRowId::RecentlyWatched {
            match db.recently_watched_row().await {
                Ok(local) => rows.push(local),
                Err(error) => {
                    warn!(%error, "failed to load recently watched");
                    rows.push(HomeRow::empty(HomeRowId::RecentlyWatched));
                }
            }
            continue;
        }

        let stale_empty = row.error.is_some() && row.items.is_empty();
        if stale_empty {
            let cached = db
                .get_json::<HomeRow>(lang, KIND_HOME, row.id.as_key())
                .await;

            if let Ok(Some(hit)) = cached {
                rows.push(hit.value);
                continue;
            }
        }

        let paths = row.image_paths();
        let saved = db
            .put_json(lang, KIND_HOME, row.id.as_key(), &row, &paths)
            .await;

        if let Err(error) = saved {
            warn!(%error, "failed to persist home row");
        }

        rows.push(row);
    }

    Ok(HomeCatalog { rows })
}

pub async fn load_media(
    tmdb: TmdbCtx,
    kind: MediaKind,
    id: TmdbId,
    db: Option<Arc<Store>>,
) -> Result<Box<MediaDetails>, JobError> {
    let mut details =
        cinebox_tmdb::fetch_media(&tmdb.api_key, kind, id, Some(tmdb.language), &tmdb.net).await?;

    if let Some(db) = db {
        let lang = language_key(Some(tmdb.language));
        let cache_id = media_cache_id(kind, id);
        let paths = details.image_paths();
        let saved = db
            .put_json(lang, KIND_MEDIA, &cache_id, &details, &paths)
            .await;

        if let Err(error) = saved {
            warn!(%error, "failed to persist media details");
        }
    }

    details.apply_typography();
    Ok(Box::new(details))
}

pub async fn load_person(
    tmdb: TmdbCtx,
    id: TmdbId,
    db: Option<Arc<Store>>,
) -> Result<Box<PersonDetails>, JobError> {
    let mut details =
        cinebox_tmdb::fetch_person(&tmdb.api_key, id, Some(tmdb.language), &tmdb.net).await?;

    if let Some(db) = db {
        let lang = language_key(Some(tmdb.language));
        let cache_id = person_cache_id(id);
        let paths = details.image_paths();
        let saved = db
            .put_json(lang, KIND_PERSON, &cache_id, &details, &paths)
            .await;

        if let Err(error) = saved {
            warn!(%error, "failed to persist person details");
        }
    }

    details.apply_typography();
    Ok(Box::new(details))
}

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

    let started = match cinebox_torrserver::list(&torr.url, &torr.username, &torr.password).await {
        Ok(rows) => rows,
        Err(error) => {
            warn!(%error, "torrserver list failed; started tags unavailable");
            Vec::new()
        }
    };

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
        .map(|hit| {
            let listing = listing_from_hit(hit);
            TorrentHit::new(listing, runtime, &hashes, &local_hashes)
        })
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
    let opened = cinebox_torrserver::open_magnet(
        &torr.url,
        &torr.username,
        &torr.password,
        &spec,
        torr.track_timecode,
    )
    .await?;

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

    let fetched =
        match cinebox_tmdb::fetch_season_episodes(&tmdb.api_key, id, &need, language, &tmdb.net)
            .await
        {
            Ok(episodes) => episodes,
            Err(error) => {
                warn!(%error, "season episode catalog failed");
                Vec::new()
            }
        };

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
            .map(format_release_date);

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

    let resume_id = rows
        .iter()
        .rev()
        .find(|file| file.timecode > 0.0)
        .map(|file| file.id);

    ReadyFiles::from_rows(opened.hash, resume_id, rows)
}

/// Wait for the stream buffer. With `resume_bytes` the wait targets a window
/// at that offset (mid-file resume); otherwise the stock head preload runs.
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
        cinebox_torrserver::wait_preload_at_bytes(
            &torr.url,
            &torr.username,
            &torr.password,
            target,
            offset,
            on_event,
        )
        .await?;

        return Ok(());
    }

    cinebox_torrserver::wait_preload(&torr.url, &torr.username, &torr.password, target, on_event)
        .await?;

    Ok(())
}

pub async fn ping_torrserver(torr: TorrCtx) -> Result<String, JobError> {
    let echo = cinebox_torrserver::echo(&torr.url, &torr.username, &torr.password).await?;

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
    let TorrCtx {
        url,
        username,
        password,
        ..
    } = torr;

    let report = cinebox_torrserver::speed_test(&url, &username, &password, on_event).await?;
    Ok(report.megabits_per_sec())
}
