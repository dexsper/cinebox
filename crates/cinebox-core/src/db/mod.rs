//! Local SQLite via SQLx: TMDB cache, playback prefs, and watch history.

mod cache;
mod prefs;
mod search;
mod types;
mod watch;

use std::io;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

use crate::catalog::{HomeCatalog, HomeRow, HomeRowId};
use crate::paths;

pub use types::{
    CONFIG_TTL, CacheHit, DETAILS_TTL, HOME_FAST_TTL, HOME_SLOW_TTL, KIND_CONFIG, KIND_HOME,
    KIND_MEDIA, KIND_PERSON, KIND_SEASON, MAX_AGE, RECENT_RELEASE_LIMIT, RECENT_ROW_LIMIT,
    SEARCH_HISTORY_LIMIT, SEASON_TTL,
    TorrentPlaybackPrefs, WatchHistoryEntry, allowed_image_sizes, home_ttl, image_size_key,
    language_key, media_cache_id, media_kind_from_key, media_kind_key, media_ttl, person_cache_id,
    season_cache_id,
};

/// Failures opening or talking to the local database.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not determine executable directory")]
    NoExeDir(#[source] io::Error),
    #[error("could not create database directory {}", .path.display())]
    CreateDir {
        path: std::path::PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("sqlite error")]
    Sqlite(#[from] sqlx::Error),
    #[error("migration failed")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("failed to serialize cache payload")]
    Serialize(#[source] serde_json::Error),
    #[error("failed to parse cache payload")]
    Deserialize(#[source] serde_json::Error),
}

/// Local app database (TMDB cache + watch history).
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Next to the executable: `cinebox.sqlite`.
    ///
    /// # Errors
    ///
    /// Missing executable dir or sqlite errors.
    pub async fn system() -> Result<Self, StoreError> {
        let dir = paths::exe_dir().map_err(StoreError::NoExeDir)?;

        Self::open(dir.join("cinebox.sqlite")).await
    }

    /// Open (or create) the database at `path` and run embedded migrations.
    ///
    /// An older rusqlite file without `_sqlx_migrations` is deleted and rebuilt.
    ///
    /// # Errors
    ///
    /// Sqlite setup or migration failures.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| StoreError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let store = connect(path).await?;
        if sqlx::migrate!("./migrations").run(&store.pool).await.is_ok() {
            return Ok(store);
        }

        store.pool.close().await;
        drop(store);
        remove_sqlite_files(path);

        let store = connect(path).await?;
        sqlx::migrate!("./migrations").run(&store.pool).await?;

        Ok(store)
    }

    /// In-memory database for tests.
    ///
    /// # Errors
    ///
    /// Sqlite setup failures.
    pub async fn memory() -> Result<Self, StoreError> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    /// Drop TMDB JSON older than six months, then collect unreferenced images.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn maintenance(&self, allowed_sizes: &[String]) -> Result<(), StoreError> {
        self.purge_expired().await?;
        self.gc_images(allowed_sizes).await
    }

    /// Home shelves from disk. `fresh` is true when every remote row is within its TTL.
    ///
    /// # Errors
    ///
    /// Sqlite or JSON parse failures.
    pub async fn home_catalog(
        &self,
        language: &str,
    ) -> Result<Option<(HomeCatalog, bool)>, StoreError> {
        let mut cached = self.home_rows(language).await?;

        let mut rows = vec![self.recently_watched_row().await?];
        let mut any = false;
        let mut fresh = true;

        for id in HomeRowId::REMOTE {
            let Some(hit) = cached.remove(id.as_key()) else {
                fresh = false;
                rows.push(HomeRow::empty(id));
                continue;
            };

            any = true;
            if !hit.is_fresh(home_ttl(id)) {
                fresh = false;
            }
            rows.push(hit.value);
        }

        if !any {
            if rows[0].items.is_empty() {
                return Ok(None);
            }

            return Ok(Some((HomeCatalog { rows }, false)));
        }

        Ok(Some((HomeCatalog { rows }, fresh)))
    }

    /// All remote home rows for `language` in one `IN` query instead of one
    /// `get_json` round-trip per shelf.
    async fn home_rows(
        &self,
        language: &str,
    ) -> Result<std::collections::HashMap<String, CacheHit<HomeRow>>, StoreError> {
        let placeholders = vec!["?"; HomeRowId::REMOTE.len()].join(", ");
        let sql = format!(
            "SELECT id, fetched_at, payload FROM tmdb_cache \
             WHERE language = ? AND kind = ? AND id IN ({placeholders})"
        );

        // The SQL is built from a fixed template plus `?` placeholders only.
        let sql = sqlx::AssertSqlSafe(sql);
        let mut query = sqlx::query_as::<_, (String, i64, String)>(sql)
            .bind(language)
            .bind(KIND_HOME);
        for id in HomeRowId::REMOTE {
            query = query.bind(id.as_key());
        }

        let rows = query.fetch_all(&self.pool).await?;

        let max_age = i64::try_from(MAX_AGE.as_secs()).unwrap_or(i64::MAX);
        let mut out = std::collections::HashMap::new();

        for (id, fetched_at, payload) in rows {
            if types::age_secs(fetched_at) >= max_age {
                continue;
            }

            let value: HomeRow =
                serde_json::from_str(&payload).map_err(StoreError::Deserialize)?;

            out.insert(id, CacheHit { value, fetched_at });
        }

        Ok(out)
    }
}

async fn connect(path: &Path) -> Result<Store, StoreError> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await?;

    Ok(Store { pool })
}

fn remove_sqlite_files(path: &Path) {
    let _ = std::fs::remove_file(path);
    let path_str = path.to_string_lossy();
    let _ = std::fs::remove_file(format!("{path_str}-wal"));
    let _ = std::fs::remove_file(format!("{path_str}-shm"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{CatalogItem, HomeRow, HomeRowId};
    use crate::ids::{MediaKind, TmdbId};
    use crate::settings::{PosterSize, VideoScale};
    use types::unix_now;

    fn sample_item(path: &str) -> CatalogItem {
        CatalogItem {
            id: TmdbId::new(1),
            kind: MediaKind::Movie,
            title: String::from("Dune"),
            year: Some(2021),
            vote: Some(8.0),
            poster_path: Some(String::from(path)),
        }
    }

    fn sample_row(path: &str) -> HomeRow {
        HomeRow {
            id: HomeRowId::NowPlaying,
            items: vec![sample_item(path)],
            error: None,
        }
    }

    fn sizes() -> Vec<String> {
        allowed_image_sizes(PosterSize::W500)
    }

    fn history_entry(id: u32, title: &str) -> WatchHistoryEntry {
        WatchHistoryEntry {
            kind: MediaKind::Movie,
            id: TmdbId::new(id),
            title: title.to_owned(),
            poster_path: Some(String::from("/p.jpg")),
            year: Some(2021),
            vote: Some(8.0),
            season: None,
            episode: None,
            episode_title: None,
            time: 12.0,
            duration: 7200.0,
        }
    }

    #[tokio::test]
    async fn json_roundtrip_is_language_scoped() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        let row = sample_row("/a.jpg");

        store
            .put_json("en-US", KIND_HOME, row.id.as_key(), &row, &row.image_paths())
            .await?;

        let got = store
            .get_json::<HomeRow>("en-US", KIND_HOME, row.id.as_key())
            .await?;
        assert!(got.is_some());

        if let Some(hit) = got {
            assert_eq!(hit.value.items[0].title, "Dune");
        }

        assert!(
            store
                .get_json::<HomeRow>("ru-RU", KIND_HOME, row.id.as_key())
                .await?
                .is_none()
        );

        Ok(())
    }

    #[tokio::test]
    async fn expired_json_is_hidden() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        let row = sample_row("/a.jpg");
        let old = unix_now() - i64::try_from(MAX_AGE.as_secs()).unwrap_or(0) - 10;

        store
            .put_json_at("", KIND_HOME, row.id.as_key(), &row, &row.image_paths(), old)
            .await?;

        assert!(
            store
                .get_json::<HomeRow>("", KIND_HOME, row.id.as_key())
                .await?
                .is_none()
        );

        Ok(())
    }

    #[tokio::test]
    async fn unreferenced_image_is_collected() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        store.put_image("w500", "/old.jpg", b"old").await?;
        store.put_image("w500", "/new.jpg", b"new").await?;
        store.set_image_accessed_at("/old.jpg", 1).await?;

        let row = sample_row("/new.jpg");
        store
            .put_json("", KIND_HOME, row.id.as_key(), &row, &row.image_paths())
            .await?;

        store.gc_images(&sizes()).await?;

        assert!(store.get_image("w500", "/old.jpg").await?.is_none());
        assert_eq!(
            store.get_image("w500", "/new.jpg").await?,
            Some(b"new".to_vec())
        );

        Ok(())
    }

    #[tokio::test]
    async fn home_catalog_reports_freshness() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        assert!(store.home_catalog("").await?.is_none());

        for id in HomeRowId::REMOTE {
            let row = HomeRow {
                id,
                items: vec![sample_item("/a.jpg")],
                error: None,
            };
            store
                .put_json("", KIND_HOME, id.as_key(), &row, &row.image_paths())
                .await?;
        }

        let got = store.home_catalog("").await?;
        assert!(got.is_some());

        if let Some((catalog, fresh)) = got {
            assert!(fresh);
            assert_eq!(catalog.rows.len(), HomeRowId::ALL.len());
        }

        Ok(())
    }

    #[tokio::test]
    async fn torrent_prefs_roundtrip() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        assert!(store.get_torrent_prefs("deadbeef").await?.is_none());

        let prefs = TorrentPlaybackPrefs {
            scale: VideoScale::Zoom130,
            speed: 1.5,
            aid: 2,
            sid: 0,
        };
        store.put_torrent_prefs("deadbeef", &prefs).await?;

        assert_eq!(store.get_torrent_prefs("deadbeef").await?, Some(prefs));
        assert!(store.get_torrent_prefs("cafebabe").await?.is_none());

        let updated = TorrentPlaybackPrefs {
            speed: 2.0,
            ..prefs
        };
        store.put_torrent_prefs("deadbeef", &updated).await?;

        assert_eq!(store.get_torrent_prefs("deadbeef").await?, Some(updated));
        Ok(())
    }

    #[tokio::test]
    async fn clear_tmdb_keeps_torrent_prefs() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        let prefs = TorrentPlaybackPrefs::default();

        store.put_torrent_prefs("deadbeef", &prefs).await?;
        store.clear_tmdb().await?;

        assert_eq!(store.get_torrent_prefs("deadbeef").await?, Some(prefs));
        Ok(())
    }

    #[test]
    fn media_cache_id_is_stable() {
        assert_eq!(
            media_cache_id(MediaKind::Movie, TmdbId::new(550)),
            "movie:550"
        );

        assert_eq!(season_cache_id(TmdbId::new(10), 2), "10:2");
    }

    #[tokio::test]
    async fn watch_timeline_roundtrip() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        let id = TmdbId::new(10);

        assert!(
            store
                .get_watch_timeline(MediaKind::Tv, id, Some(1), Some(2))
                .await?
                .is_none()
        );

        store
            .upsert_watch_timeline(MediaKind::Tv, id, Some(1), Some(2), 15.5, 2400.0)
            .await?;

        assert_eq!(
            store
                .get_watch_timeline(MediaKind::Tv, id, Some(1), Some(2))
                .await?,
            Some((15.5, 2400.0))
        );
        assert!(
            store
                .get_watch_timeline(MediaKind::Tv, id, Some(1), Some(3))
                .await?
                .is_none()
        );

        store
            .upsert_watch_timeline(MediaKind::Movie, TmdbId::new(7), None, None, 90.0, 7200.0)
            .await?;

        assert_eq!(
            store
                .get_watch_timeline(MediaKind::Movie, TmdbId::new(7), None, None)
                .await?,
            Some((90.0, 7200.0))
        );

        Ok(())
    }

    #[tokio::test]
    async fn recently_watched_is_mru_and_one_per_media() -> Result<(), StoreError> {
        let store = Store::memory().await?;

        store
            .upsert_watch_history(&history_entry(1, "First"), Some("deadbeef"))
            .await?;
        store
            .upsert_watch_history(&history_entry(2, "Second"), None)
            .await?;
        store
            .upsert_watch_history(&history_entry(1, "First again"), Some("deadbeef"))
            .await?;

        let recent = store.recently_watched(20).await?;
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, TmdbId::new(1));
        assert_eq!(recent[0].title, "First again");
        assert_eq!(recent[1].id, TmdbId::new(2));

        let keys = store.watched_keys().await?;
        assert_eq!(keys.len(), 2);

        assert_eq!(
            store
                .watch_release_hashes(MediaKind::Movie, TmdbId::new(1))
                .await?,
            vec![String::from("deadbeef")]
        );

        let row = store.recently_watched_row().await?;
        assert_eq!(row.id, HomeRowId::RecentlyWatched);
        assert_eq!(row.items.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn watch_release_keeps_three_newest_hashes() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        let id = TmdbId::new(1);

        store
            .upsert_watch_history(&history_entry(1, "A"), Some("hash1"))
            .await?;
        store
            .upsert_watch_history(&history_entry(1, "A"), Some("hash2"))
            .await?;
        store
            .upsert_watch_history(&history_entry(1, "A"), Some("hash3"))
            .await?;
        store
            .upsert_watch_history(&history_entry(1, "A"), Some("hash4"))
            .await?;

        let hashes = store.watch_release_hashes(MediaKind::Movie, id).await?;
        assert_eq!(
            hashes,
            vec![
                String::from("hash4"),
                String::from("hash3"),
                String::from("hash2")
            ]
        );

        Ok(())
    }

    #[tokio::test]
    async fn clear_tmdb_keeps_watch_history() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        store
            .upsert_watch_history(&history_entry(9, "Kept"), None)
            .await?;
        store.clear_tmdb().await?;

        let recent = store.recently_watched(10).await?;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, TmdbId::new(9));

        Ok(())
    }

    #[tokio::test]
    async fn search_history_is_mru_capped_and_case_insensitive() -> Result<(), StoreError> {
        let store = Store::memory().await?;

        store.record_search("   ").await?;
        assert!(store.recent_searches(10).await?.is_empty());

        store.record_search("Dune").await?;
        store.record_search("Alien").await?;
        store.record_search("dune").await?;

        let recent = store.recent_searches(10).await?;
        assert_eq!(recent, vec![String::from("dune"), String::from("Alien")]);

        for index in 0..SEARCH_HISTORY_LIMIT + 2 {
            let query = format!("q{index}");
            store.record_search(&query).await?;
        }

        let kept = store.recent_searches(20).await?;
        assert_eq!(kept.len(), SEARCH_HISTORY_LIMIT);
        assert_eq!(kept[0], format!("q{}", SEARCH_HISTORY_LIMIT + 1));
        assert!(!kept.iter().any(|query| query == "dune"));

        Ok(())
    }

    #[tokio::test]
    async fn clear_tmdb_keeps_search_history() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        store.record_search("Dune").await?;
        store.clear_tmdb().await?;

        let recent = store.recent_searches(10).await?;
        assert_eq!(recent, vec![String::from("Dune")]);

        Ok(())
    }
}
