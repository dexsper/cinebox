//! Async jobs spawned on the egui-async Tokio runtime.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use cinebox_core::{
    CONFIG_TTL, HomeCatalog, HomeRow, HomeRowId, KIND_CONFIG, KIND_HOME, KIND_MEDIA, KIND_PERSON,
    KIND_SEASON, MediaDetails, MediaKind, PersonDetails, SEASON_TTL, Settings, Store, TmdbId,
    allowed_image_sizes, format_release_date, language_key, media_cache_id, normalize_tmdb_path,
    person_cache_id, season_cache_id, tmdb_image_url,
};
use cinebox_parse::{
    Listing, SortMode, TorrentHit, file_display_name, parse_file_episode, sort_hits,
};
use tracing::warn;

use crate::screens::torrents::{MovieBits, ReadyFiles, TorrentFileRow};

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
    settings: Settings,
    id: HomeRowId,
    page: u32,
) -> Result<cinebox_tmdb::CatalogPage, String> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.general.language.tmdb_code();
    let use_system_proxy = settings.general.use_system_proxy;

    cinebox_tmdb::fetch_catalog_page(&key, id, page, Some(language), use_system_proxy)
        .await
        .map_err(|error| error.to_string())
}

pub async fn load_home(settings: Settings, db: Option<Arc<Store>>) -> Result<HomeCatalog, String> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.general.language.tmdb_code();
    let use_system_proxy = settings.general.use_system_proxy;

    let fetched = cinebox_tmdb::fetch_home(&key, Some(language), use_system_proxy)
        .await
        .map_err(|error| error.to_string())?;

    let Some(db) = db else {
        return Ok(fetched);
    };
    let lang = language_key(Some(language));
    let sizes = allowed_image_sizes(settings.tmdb.poster_size);
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
            if let Ok(Some(hit)) = db.get_json::<HomeRow>(lang, KIND_HOME, row.id.as_key()).await {
                rows.push(hit.value);
                continue;
            }
        }
        let paths = row.image_paths();
        if let Err(error) = db
            .put_json(lang, KIND_HOME, row.id.as_key(), &row, &paths, &sizes)
            .await
        {
            warn!(%error, "failed to persist home row");
        }
        rows.push(row);
    }
    Ok(HomeCatalog { rows })
}

pub async fn load_media(
    settings: Settings,
    kind: MediaKind,
    id: TmdbId,
    db: Option<Arc<Store>>,
) -> Result<Box<MediaDetails>, String> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.general.language.tmdb_code();
    let use_system_proxy = settings.general.use_system_proxy;

    let mut details = cinebox_tmdb::fetch_media(&key, kind, id, Some(language), use_system_proxy)
        .await
        .map_err(|error| error.to_string())?;

    if let Some(db) = db {
        let lang = language_key(Some(language));
        let sizes = allowed_image_sizes(settings.tmdb.poster_size);
        let cache_id = media_cache_id(kind, id);
        let paths = details.image_paths();
        if let Err(error) = db
            .put_json(lang, KIND_MEDIA, &cache_id, &details, &paths, &sizes)
            .await
        {
            warn!(%error, "failed to persist media details");
        }
    }

    details.apply_typography();
    Ok(Box::new(details))
}

pub async fn load_person(
    settings: Settings,
    id: TmdbId,
    db: Option<Arc<Store>>,
) -> Result<Box<PersonDetails>, String> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.general.language.tmdb_code();
    let use_system_proxy = settings.general.use_system_proxy;

    let mut details = cinebox_tmdb::fetch_person(&key, id, Some(language), use_system_proxy)
        .await
        .map_err(|error| error.to_string())?;

    if let Some(db) = db {
        let lang = language_key(Some(language));
        let sizes = allowed_image_sizes(settings.tmdb.poster_size);
        let cache_id = person_cache_id(id);
        let paths = details.image_paths();
        if let Err(error) = db
            .put_json(lang, KIND_PERSON, &cache_id, &details, &paths, &sizes)
            .await
        {
            warn!(%error, "failed to persist person details");
        }
    }

    details.apply_typography();
    Ok(Box::new(details))
}

pub async fn load_torrents(
    settings: Settings,
    details: MediaDetails,
    db: Option<Arc<Store>>,
) -> Result<Vec<TorrentHit>, String> {
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
    let parser_kind = settings.parser.kind;
    let parser_url = settings.parser.url.clone();
    let parser_key = settings.parser.api_key.expose().to_owned();
    let use_system_proxy = settings.general.use_system_proxy;
    let ts_url = settings.torrserver.url.clone();
    let ts_user = settings.torrserver.username.clone();
    let ts_pass = settings.torrserver.password.expose().to_owned();

    let raw = cinebox_indexer::search(
        parser_kind,
        &parser_url,
        &parser_key,
        &query,
        use_system_proxy,
    )
    .await
    .map_err(|error| error.to_string())?;

    let started = match cinebox_torrserver::list(&ts_url, &ts_user, &ts_pass).await {
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
    settings: Settings,
    spec: cinebox_torrserver::AddSpec,
    movie: MovieBits,
    kind: MediaKind,
    id: TmdbId,
    runtime_minutes: Option<u32>,
    db: Option<Arc<Store>>,
) -> Result<ReadyFiles, String> {
    let url = settings.torrserver.url.clone();
    let user = settings.torrserver.username.clone();
    let pass = settings.torrserver.password.expose().to_owned();
    let track = settings.torrserver.track_timecode;
    let api_key = settings.tmdb.api_key.expose().to_owned();

    let opened = cinebox_torrserver::open_magnet(&url, &user, &pass, &spec, track)
        .await
        .map_err(|error| error.to_string())?;

    let serial = kind == MediaKind::Tv || movie.number_of_seasons.is_some();
    let catalog = season_catalog(
        &opened.files,
        serial,
        id,
        &api_key,
        &settings,
        db.clone(),
    )
    .await;

    let files = decorate_files(
        opened,
        &movie,
        serial,
        runtime_minutes,
        &catalog,
        db.as_deref().map(|db| (db, kind, id)),
    )
    .await;

    Ok(files)
}

async fn season_catalog(
    files: &[cinebox_torrserver::OpenedFile],
    serial: bool,
    id: TmdbId,
    api_key: &str,
    settings: &Settings,
    db: Option<Arc<Store>>,
) -> Vec<cinebox_tmdb::SeasonEpisode> {
    if !serial || api_key.is_empty() {
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

    let language = Some(settings.general.language.tmdb_code());
    let lang = language_key(language);
    let sizes = allowed_image_sizes(settings.tmdb.poster_size);
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

    let fetched = match cinebox_tmdb::fetch_season_episodes(
        api_key,
        id,
        &need,
        language,
        settings.general.use_system_proxy,
    )
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
            if let Err(error) = db
                .put_json(lang, KIND_SEASON, &cache_id, &eps, &paths, &sizes)
                .await
            {
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

pub async fn wait_stream(
    settings: Settings,
    file_path: String,
    hash: String,
    file_id: i32,
    on_event: impl FnMut(cinebox_torrserver::PreloadEvent) + Send,
) -> Result<(), String> {
    let url = settings.torrserver.url.clone();
    let user = settings.torrserver.username.clone();
    let pass = settings.torrserver.password.expose().to_owned();

    cinebox_torrserver::wait_preload(&url, &user, &pass, &file_path, &hash, file_id, on_event)
        .await
        .map_err(|error| error.to_string())
}

pub async fn ping_torrserver(settings: Settings) -> Result<String, String> {
    cinebox_torrserver::echo(
        &settings.torrserver.url,
        &settings.torrserver.username,
        settings.torrserver.password.expose(),
    )
    .await
    .map_err(|error| error.to_string())
}

pub async fn ping_parser(settings: Settings) -> Result<String, String> {
    cinebox_indexer::ping(
        settings.parser.kind,
        &settings.parser.url,
        settings.parser.api_key.expose(),
        settings.general.use_system_proxy,
    )
    .await
    .map_err(|error| error.to_string())
}

pub async fn ping_tmdb(settings: Settings, db: Option<Arc<Store>>) -> Result<String, String> {
    let fp = key_fingerprint(settings.tmdb.api_key.expose());
    let cache_id = format!("ping:{fp:x}");
    if let Some(db) = &db
        && let Ok(Some(hit)) = db.get_json::<String>("", KIND_CONFIG, &cache_id).await
        && hit.is_fresh(CONFIG_TTL)
    {
        return Ok(hit.value);
    }

    let result = cinebox_tmdb::check_api_key(
        settings.tmdb.api_key.expose(),
        settings.general.use_system_proxy,
    )
    .await
    .map_err(|error| error.to_string());

    if let Ok(msg) = &result
        && let Some(db) = db
        && let Err(error) = db
            .put_json(
                "",
                KIND_CONFIG,
                &cache_id,
                msg,
                &[],
                &allowed_image_sizes(settings.tmdb.poster_size),
            )
            .await
        {
        warn!(%error, "failed to persist tmdb ping");
    }
    result
}

fn key_fingerprint(key: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

pub async fn speed_test(
    settings: Settings,
    on_event: impl FnMut(cinebox_torrserver::SpeedEvent) + Send,
) -> Result<f64, String> {
    cinebox_torrserver::speed_test(
        &settings.torrserver.url,
        &settings.torrserver.username,
        settings.torrserver.password.expose(),
        on_event,
    )
    .await
    .map(|report| report.megabits_per_sec())
    .map_err(|error| error.to_string())
}
