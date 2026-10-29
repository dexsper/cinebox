//! Takes in the torrents, cards and positions other clients leave on TorrServer (`sync_history`).
//!
//! Every torrent with a TMDB title joins the recently watched history, found through its card in `data`,
//! a match remembered from before, or a name search. Positions another client moved since this device last
//! saw them are taken in. A torrent found by name gets a card in its `data`, so other clients show it too.
//! Torrents gone from the server stay in the history.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cinebox_core::{
    CacheKey, KIND_MEDIA, MediaDetails, MediaKind, ServerMatch, Store, TmdbId, WatchHistoryEntry, WatchKey,
    language_key, media_cache_id, media_ttl,
};
use cinebox_indexer::{name_candidates, parse_title};
use cinebox_tmdb::{SearchKind, SearchQuery, TitleMatch, fetch_title_matches};
use cinebox_torrserver::{
    FileStat, ListedTorrent, MetaSpec, TorrCard, Viewed, decode_data, is_foreign_timecode, parse_file_episode,
    set_meta, stored_files, with_card,
};
use tracing::warn;

use super::{JobError, TmdbCtx, TorrCtx, load_media};
use crate::screens::play::WatchCard;

/// A sync starts at most this often.
pub(crate) const SYNC_INTERVAL: Duration = Duration::from_mins(2);

/// TMDB name searches one sync may spend on torrents without a card.
const MAX_SEARCHES: usize = 24;

/// Names of one torrent tried in a search.
const MAX_CANDIDATES: usize = 3;

/// What one sync changed on this device.
#[derive(Debug, Default)]
pub(crate) struct SyncOutcome {
    /// The local history changed, so the shelves that show it are stale.
    pub changed: bool,

    /// Titles another client started, for the watched badge and the library.
    pub started: Vec<WatchCard>,
}

/// A position taken in from `/viewed`.
#[derive(Debug, Clone, Copy)]
struct Imported {
    key: WatchKey,
    file_index: i32,
    time: f64,
}

impl Imported {
    /// Later episodes sort after earlier ones; within one, the further position sorts last.
    fn order(&self) -> (u32, u32, f64) {
        (self.key.season.unwrap_or(0), self.key.episode.unwrap_or(0), self.time)
    }
}

/// State of one sync over the server's torrents.
struct SyncPass<'a> {
    torr: &'a TorrCtx,
    tmdb: &'a TmdbCtx,
    db: &'a Arc<Store>,
    searches_left: usize,
    outcome: SyncOutcome,
}

impl<'a> SyncPass<'a> {
    fn new(torr: &'a TorrCtx, tmdb: &'a TmdbCtx, db: &'a Arc<Store>) -> Self {
        Self { torr, tmdb, db, searches_left: MAX_SEARCHES, outcome: SyncOutcome::default() }
    }

    async fn torrent(&mut self, torrent: &ListedTorrent, positions: &[&Viewed]) -> Result<(), JobError> {
        let card = decode_data(&torrent.data);
        let Some((kind, id)) = self.title_of(torrent, card.as_ref()).await? else { return Ok(()) };

        let details = cached_media(self.tmdb, self.db, kind, id).await?;

        if card.is_none() {
            self.write_card(torrent, &details).await;
        }

        let latest = self.import_positions(torrent, positions, kind, id).await?;

        self.import_history(torrent, &details, latest).await
    }

    async fn title_of(
        &mut self,
        torrent: &ListedTorrent,
        card: Option<&TorrCard>,
    ) -> Result<Option<(MediaKind, TmdbId)>, JobError> {
        let known = self.db.torrserver_match(&torrent.hash).await?;

        if let Some((kind, id)) = card.and_then(TorrCard::tmdb_ref) {
            if known != ServerMatch::Found(kind, id) {
                self.db.put_torrserver_match(&torrent.hash, Some((kind, id))).await?;
            }

            return Ok(Some((kind, id)));
        }

        match known {
            ServerMatch::Found(kind, id) => return Ok(Some((kind, id))),
            ServerMatch::NotFound => return Ok(None),
            ServerMatch::Unknown => {}
        }

        let Some(found) = self.search(torrent).await? else { return Ok(None) };

        self.db.put_torrserver_match(&torrent.hash, found).await?;

        Ok(found)
    }

    /// `None` when the searches left in this sync are too few to look the torrent up; it is tried next time.
    async fn search(&mut self, torrent: &ListedTorrent) -> Result<Option<Option<(MediaKind, TmdbId)>>, JobError> {
        let info = parse_title(&torrent.title);
        let kinds = search_kinds(&torrent.category, info.seasons_named);
        let names = name_candidates(&torrent.title);
        let names = &names[..names.len().min(MAX_CANDIDATES)];

        let cost = names.len() * kinds.len();
        if cost > self.searches_left {
            return Ok(None);
        }

        self.searches_left -= cost;

        for name in names {
            for kind in kinds {
                let query = SearchQuery { text: name, kind: *kind, page: 1 };
                let language = Some(self.tmdb.language);
                let found = fetch_title_matches(&self.tmdb.api_key, query, language, &self.tmdb.net).await?;

                if let Some(title) = found.iter().find(|found| is_same_title(found, name, info.year)) {
                    return Ok(Some(Some((title.item.kind, title.item.id))));
                }
            }
        }

        Ok(Some(None))
    }

    /// Puts a card into a torrent's `data` that has none. A failure only costs other clients the card.
    async fn write_card(&self, torrent: &ListedTorrent, details: &MediaDetails) {
        let card = TorrCard::from_details(details, &torrent.poster);
        let data = with_card(&torrent.data, &card);
        let title = if torrent.title.is_empty() { details.title.as_str() } else { torrent.title.as_str() };
        let poster = if torrent.poster.is_empty() { card.img.as_str() } else { torrent.poster.as_str() };
        let category = if torrent.category.is_empty() { card.category() } else { torrent.category.as_str() };
        let meta = MetaSpec { hash: &torrent.hash, title, poster, category, data: &data };

        if let Err(error) = set_meta(&self.torr.server, meta).await {
            warn!(%error, hash = torrent.hash, "failed to write a card to a torrserver torrent");
        }
    }

    /// Takes in the positions of `torrent` another client moved, and returns the latest of them.
    async fn import_positions(
        &self,
        torrent: &ListedTorrent,
        positions: &[&Viewed],
        kind: MediaKind,
        id: TmdbId,
    ) -> Result<Option<Imported>, JobError> {
        if positions.is_empty() {
            return Ok(None);
        }

        let seen = self.db.viewed_seen(&torrent.hash).await?;
        let stored = stored_files(&torrent.data);
        let files = if torrent.file_stats.is_empty() { stored.as_slice() } else { torrent.file_stats.as_slice() };

        let mut imports = Vec::new();
        for row in positions {
            let seen_here = seen.iter().find(|(index, _)| *index == row.file_index).map(|(_, timecode)| *timecode);
            if !is_foreign_timecode(row.timecode, seen_here) {
                continue;
            }

            let Some(key) = watch_key(kind, id, files, row.file_index) else {
                continue;
            };

            imports.push(Imported { key, file_index: row.file_index, time: row.timecode });
        }

        // The latest goes last, so it is the one a movie with several files keeps.
        imports.sort_by(|left, right| left.order().partial_cmp(&right.order()).unwrap_or(std::cmp::Ordering::Equal));

        for import in &imports {
            self.db.import_watch_timeline(import.key, import.time).await?;
            self.db.put_viewed_seen(&torrent.hash, import.file_index, import.time).await?;
        }

        Ok(imports.last().copied())
    }

    async fn import_history(
        &mut self,
        torrent: &ListedTorrent,
        details: &MediaDetails,
        latest: Option<Imported>,
    ) -> Result<(), JobError> {
        let card = WatchCard::from_details(details);
        let updated_at = if latest.is_some() { unix_now().max(torrent.timestamp) } else { torrent.timestamp };
        let entry = WatchHistoryEntry {
            kind: card.kind,
            id: card.id,
            section: card.section,
            title: card.title.clone(),
            poster_path: card.poster_path.clone(),
            year: card.year,
            vote: card.vote,
            season: latest.and_then(|import| import.key.season),
            episode: latest.and_then(|import| import.key.episode),
            episode_title: None,
            time: latest.map_or(0.0, |import| import.time),
            duration: 0.0,
        };

        let written = self.db.import_watch_history(&entry, &torrent.hash, updated_at).await?;
        self.outcome.changed |= written;

        if written && latest.is_some() {
            self.outcome.started.push(card);
        }

        Ok(())
    }
}

/// One sync with the TorrServer in `torr`. Does nothing, and sends no request, unless `sync_history` is on.
///
/// A torrent that fails is logged and skipped; the error returned is a failure to read the server.
pub(crate) async fn sync_from_server(
    torr: TorrCtx,
    tmdb: TmdbCtx,
    db: Option<Arc<Store>>,
) -> Result<SyncOutcome, JobError> {
    let Some(db) = db.filter(|_| torr.sync_history) else { return Ok(SyncOutcome::default()) };

    // Titles are named through TMDB; without a key the home screen asks for one first.
    if tmdb.api_key.is_empty() {
        return Ok(SyncOutcome::default());
    }

    db.bind_torrserver(&torr.server.url).await?;

    let torrents = cinebox_torrserver::list(&torr.server).await?;
    let viewed = cinebox_torrserver::viewed_all(&torr.server).await?;
    let mut pass = SyncPass::new(&torr, &tmdb, &db);

    for torrent in &torrents {
        let rows = viewed.iter().filter(|row| row.hash.eq_ignore_ascii_case(&torrent.hash));
        let positions: Vec<&Viewed> = rows.collect();

        if let Err(error) = pass.torrent(torrent, &positions).await {
            warn!(%error, hash = torrent.hash, "torrserver sync skipped a torrent");
        }
    }

    Ok(pass.outcome)
}

/// Details from the cache while fresh, else from TMDB (and cached).
async fn cached_media(tmdb: &TmdbCtx, db: &Arc<Store>, kind: MediaKind, id: TmdbId) -> Result<MediaDetails, JobError> {
    let language = language_key(Some(tmdb.language));
    let cache_id = media_cache_id(kind, id);
    let key = CacheKey { language, kind: KIND_MEDIA, id: &cache_id };
    let cached = db.get_json::<MediaDetails>(key).await.ok().flatten();

    if let Some(hit) = cached.filter(|hit| hit.is_fresh(media_ttl(&hit.value))) {
        return Ok(hit.value);
    }

    let details = load_media(tmdb.clone(), kind, id, Some(Arc::clone(db))).await?;

    Ok(*details)
}

/// Where to look a torrent up, from its category or, without one, its name.
fn search_kinds(category: &str, is_series: bool) -> &'static [SearchKind] {
    match category {
        "movie" => &[SearchKind::Movie],
        "tv" => &[SearchKind::Tv],
        "music" | "other" => &[],
        _ if is_series => &[SearchKind::Tv, SearchKind::Movie],
        _ => &[SearchKind::Movie, SearchKind::Tv],
    }
}

/// Only a sure match: the same name, local or original, and a release year that fits the torrent's.
fn is_same_title(found: &TitleMatch, name: &str, year: Option<u16>) -> bool {
    let wanted = comparable(name);
    let original = found.original_title.as_deref().map(comparable);
    let is_named = comparable(&found.item.title) == wanted || original.as_deref() == Some(wanted.as_str());

    if !is_named {
        return false;
    }

    let Some(year) = year else { return true };
    let Some(released) = found.item.year else { return false };

    // A release of a later season comes out years after the show started; a movie may be dated a year off.
    match found.item.kind {
        MediaKind::Tv => released <= year.saturating_add(1),
        MediaKind::Movie | MediaKind::Person => released.abs_diff(year) <= 1,
    }
}

/// Lowercase words without punctuation, `ё` read as `е`.
fn comparable(text: &str) -> String {
    let lower = text.to_lowercase().replace('ё', "е");
    let spaced: String = lower.chars().map(|ch| if ch.is_alphanumeric() { ch } else { ' ' }).collect();

    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The local key of a torrent file: the movie itself, or the episode its path names.
fn watch_key(kind: MediaKind, id: TmdbId, files: &[FileStat], file_index: i32) -> Option<WatchKey> {
    if kind != MediaKind::Tv {
        return Some(WatchKey { kind, id, season: None, episode: None });
    }

    let file = files.iter().find(|file| file.id == file_index)?;
    let parsed = parse_file_episode(&file.path, true);
    let episode = parsed.episode?;

    Some(WatchKey { kind, id, season: parsed.season, episode: Some(episode) })
}

fn unix_now() -> i64 {
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();

    i64::try_from(since_epoch.as_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use cinebox_core::{CatalogItem, Settings};

    use super::*;
    use crate::services::db_block_on;

    fn found(kind: MediaKind, title: &str, original: Option<&str>, year: u16) -> TitleMatch {
        let item = CatalogItem {
            id: TmdbId::new(1),
            kind,
            title: title.to_owned(),
            year: Some(year),
            vote: None,
            poster_path: None,
        };

        TitleMatch { item, original_title: original.map(str::to_owned) }
    }

    #[test]
    fn a_local_or_original_name_of_the_same_year_matches() {
        let dune = found(MediaKind::Movie, "Дюна", Some("Dune"), 2021);

        assert!(is_same_title(&dune, "Dune", Some(2021)));
        assert!(is_same_title(&dune, "дюна", Some(2022)));
        assert!(is_same_title(&dune, "Dune", None));
    }

    #[test]
    fn another_name_or_year_does_not_match() {
        let dune = found(MediaKind::Movie, "Дюна", Some("Dune"), 2021);

        assert!(!is_same_title(&dune, "Dune Part Two", Some(2021)));
        assert!(!is_same_title(&dune, "Dune", Some(1984)));
    }

    #[test]
    fn a_later_season_matches_the_show() {
        let show = found(MediaKind::Tv, "Ёлки", None, 2019);

        assert!(is_same_title(&show, "Елки", Some(2023)));
        assert!(!is_same_title(&show, "Елки", Some(2010)));
    }

    #[test]
    fn a_show_file_is_keyed_by_its_episode() {
        let files = [FileStat { id: 2, path: String::from("Show/Show.S02E05.mkv"), length: 1 }];

        let key = watch_key(MediaKind::Tv, TmdbId::new(5), &files, 2);

        assert_eq!(key, Some(WatchKey { kind: MediaKind::Tv, id: TmdbId::new(5), season: Some(2), episode: Some(5) }));
        assert_eq!(watch_key(MediaKind::Tv, TmdbId::new(5), &files, 3), None);
    }

    #[test]
    fn a_disabled_sync_sends_no_request() -> Result<(), JobError> {
        let server = httpmock::MockServer::start();
        let any = server.mock(|when, then| {
            when.any_request();
            then.status(200).body("[]");
        });

        let mut settings = Settings::default();
        settings.torrserver.url = server.base_url();
        settings.tmdb.api_key = cinebox_core::SecretString::from("0123456789abcdef0123456789abcdef");
        let db = Arc::new(db_block_on(Store::memory())?);

        let outcome = db_block_on(sync_from_server(TorrCtx::from(&settings), TmdbCtx::from(&settings), Some(db)))?;

        assert!(!outcome.changed);
        assert_eq!(any.calls(), 0);

        Ok(())
    }
}
