//! Async iced tasks: TMDB, indexer search, TorrServer started hashes.

use cinebox_core::{
    MediaDetails, MediaKind, Settings, TmdbId, format_release_date, tmdb_image_url,
};
use cinebox_parse::{
    Listing, SortMode, TorrentHit, file_display_name, parse_file_episode, sort_hits,
};
use iced::Task;

use crate::message::Message;
use crate::ui::torrents::{MovieBits, ReadyFiles, TorrentFileRow};

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

pub(crate) fn load_home_task(settings: &Settings) -> Task<Message> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.tmdb.data_language.clone();
    let use_system_proxy = settings.interface.use_system_proxy;

    Task::perform(
        async move {
            cinebox_tmdb::fetch_home(&key, language.as_deref(), use_system_proxy)
                .await
                .map_err(|error| error.to_string())
        },
        Message::HomeLoaded,
    )
}

pub(crate) fn load_media_task(settings: &Settings, kind: MediaKind, id: TmdbId) -> Task<Message> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.tmdb.data_language.clone();
    let use_system_proxy = settings.interface.use_system_proxy;

    Task::perform(
        async move {
            cinebox_tmdb::fetch_media(&key, kind, id, language.as_deref(), use_system_proxy)
                .await
                .map(Box::new)
                .map_err(|error| error.to_string())
        },
        move |result| Message::MediaLoaded { kind, id, result },
    )
}

pub(crate) fn load_person_task(settings: &Settings, id: TmdbId) -> Task<Message> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.tmdb.data_language.clone();
    let use_system_proxy = settings.interface.use_system_proxy;

    Task::perform(
        async move {
            cinebox_tmdb::fetch_person(&key, id, language.as_deref(), use_system_proxy)
                .await
                .map(Box::new)
                .map_err(|error| error.to_string())
        },
        move |result| Message::PersonLoaded { id, result },
    )
}

pub(crate) fn load_torrents_task(settings: &Settings, details: &MediaDetails) -> Task<Message> {
    let kind = details.kind;
    let id = details.id;
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
    let use_system_proxy = settings.interface.use_system_proxy;
    let ts_url = settings.torrserver.url.clone();
    let ts_user = settings.torrserver.username.clone();
    let ts_pass = settings.torrserver.password.expose().to_owned();
    let preferred = settings.player.default_quality;

    Task::perform(
        async move {
            let raw = cinebox_indexer::search(
                parser_kind,
                &parser_url,
                &parser_key,
                &query,
                use_system_proxy,
            )
            .await
            .map_err(|error| error.to_string())?;
            let started = cinebox_torrserver::list(&ts_url, &ts_user, &ts_pass)
                .await
                .unwrap_or_default();
            let hashes: Vec<String> = started.into_iter().map(|row| row.hash).collect();
            let mut hits: Vec<TorrentHit> = raw
                .into_iter()
                .map(|hit| TorrentHit::new(listing_from_hit(hit), runtime, &hashes))
                .collect();
            sort_hits(&mut hits, kind, preferred, SortMode::Popular);
            Ok(hits)
        },
        move |result| Message::TorrentsLoaded { kind, id, result },
    )
}

pub(crate) fn open_magnet_task(
    settings: &Settings,
    spec: cinebox_torrserver::AddSpec,
    movie: MovieBits,
    kind: MediaKind,
    id: TmdbId,
    runtime_minutes: Option<u32>,
    seq: u64,
) -> Task<Message> {
    let url = settings.torrserver.url.clone();
    let user = settings.torrserver.username.clone();
    let pass = settings.torrserver.password.expose().to_owned();
    let track = settings.torrserver.track_timecode;
    let api_key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.tmdb.data_language.clone();
    let use_system_proxy = settings.interface.use_system_proxy;

    Task::perform(
        async move {
            let opened = cinebox_torrserver::open_magnet(&url, &user, &pass, &spec, track)
                .await
                .map_err(|error| error.to_string())?;

            let is_tv = kind == MediaKind::Tv;
            let has_seasons = movie.number_of_seasons.is_some();
            let serial = is_tv || has_seasons;
            let catalog = season_catalog(
                &opened.files,
                serial,
                id,
                &api_key,
                language.as_deref(),
                use_system_proxy,
            )
            .await;

            Ok(decorate_files(
                opened,
                &movie,
                serial,
                runtime_minutes,
                &catalog,
            ))
        },
        move |result| Message::TorrentOpened {
            kind,
            id,
            seq,
            result,
        },
    )
}

async fn season_catalog(
    files: &[cinebox_torrserver::OpenedFile],
    serial: bool,
    id: TmdbId,
    api_key: &str,
    language: Option<&str>,
    use_system_proxy: bool,
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

    cinebox_tmdb::fetch_season_episodes(api_key, id, &seasons, language, use_system_proxy)
        .await
        .unwrap_or_default()
}

fn decorate_files(
    opened: cinebox_torrserver::OpenedTorrent,
    movie: &MovieBits,
    serial: bool,
    runtime_minutes: Option<u32>,
    catalog: &[cinebox_tmdb::SeasonEpisode],
) -> ReadyFiles {
    let fallback_still = tmdb_image_url(movie.backdrop_path.as_deref(), "w300")
        .or_else(|| tmdb_image_url(movie.poster_path.as_deref(), "w300"));

    let mut rows = Vec::new();
    for (index, file) in opened.files.into_iter().enumerate() {
        let parsed = parse_file_episode(&file.path, serial);
        let tmdb = catalog.iter().find(|ep| {
            let same_season = parsed.season == Some(ep.season);
            let same_episode = parsed.episode == Some(ep.episode);
            same_season && same_episode
        });

        let human = file_display_name(&file.path);
        let named = tmdb.filter(|ep| !ep.name.is_empty());
        let title = if let Some(ep) = named {
            ep.name.clone()
        } else if serial {
            human
        } else {
            movie.title.clone()
        };

        let still_url = tmdb
            .and_then(|ep| tmdb_image_url(ep.still_path.as_deref(), "w300"))
            .or_else(|| fallback_still.clone());

        let runtime = tmdb.and_then(|ep| ep.runtime_minutes).or(runtime_minutes);
        let number = parsed.episode.unwrap_or((index as u32).saturating_add(1));
        let air_date = tmdb
            .and_then(|ep| ep.air_date.as_deref())
            .filter(|d| !d.is_empty())
            .map(format_release_date);

        rows.push(TorrentFileRow {
            id: file.id,
            path: file.path,
            length: file.length,
            timecode: file.timecode,
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

    ReadyFiles::from_rows(opened.hash, opened.resume_id, rows)
}

pub(crate) fn wait_stream_task(
    settings: &Settings,
    file_path: String,
    hash: String,
    file_id: i32,
    kind: MediaKind,
    id: TmdbId,
    seq: u64,
) -> Task<Message> {
    let url = settings.torrserver.url.clone();
    let user = settings.torrserver.username.clone();
    let pass = settings.torrserver.password.expose().to_owned();
    let wait = settings.torrserver.wait_preload;

    Task::perform(
        async move {
            if wait {
                cinebox_torrserver::wait_preload(&url, &user, &pass, &file_path, &hash, file_id)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            cinebox_torrserver::stream_url(
                &url,
                &file_path,
                &hash,
                file_id,
                cinebox_torrserver::StreamFlag::Play,
            )
            .map_err(|error| error.to_string())
        },
        move |result| Message::StreamReady {
            kind,
            id,
            seq,
            file_id,
            result,
        },
    )
}
