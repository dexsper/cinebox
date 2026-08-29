//! Async iced tasks: TMDB, indexer search, TorrServer started hashes.

use cinebox_core::{MediaDetails, MediaKind, Settings, TmdbId};
use cinebox_parse::{Listing, SortMode, TorrentHit, sort_hits};
use iced::Task;

use crate::message::Message;

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
