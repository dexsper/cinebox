//! Local SQLite: TMDB JSON/image cache now, watch history later.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::catalog::{HomeCatalog, HomeRow, HomeRowId, normalize_tmdb_path};
use crate::ids::{MediaKind, TmdbId};
use crate::media::MediaDetails;
use crate::paths;
use crate::settings::PosterSize;

/// TMDB content older than this must be dropped (API ToS).
pub const MAX_AGE: Duration = Duration::from_secs(183 * 24 * 3600);
/// Home `now_playing` / `trending/day`.
pub const HOME_FAST_TTL: Duration = Duration::from_secs(3 * 3600);
/// Home popular / top rated / trending week.
pub const HOME_SLOW_TTL: Duration = Duration::from_secs(18 * 3600);
/// Media / person / in-progress TV seasons.
pub const DETAILS_TTL: Duration = Duration::from_secs(24 * 3600);
/// Released movie details.
pub const DETAILS_STABLE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
/// Season episode lists.
pub const SEASON_TTL: Duration = Duration::from_secs(24 * 3600);
/// `/configuration` API-key probe.
pub const CONFIG_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

const IMAGE_GC_GRACE: Duration = Duration::from_secs(5 * 60);
const IMAGE_BUDGET_BYTES: i64 = 512 * 1024 * 1024;

pub const KIND_HOME: &str = "home";
pub const KIND_MEDIA: &str = "media";
pub const KIND_PERSON: &str = "person";
pub const KIND_SEASON: &str = "season";
pub const KIND_CONFIG: &str = "config";

const SCHEMA_V1: &str = "
CREATE TABLE tmdb_cache (
    language TEXT NOT NULL,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (language, kind, id)
);
CREATE TABLE tmdb_image (
    size TEXT NOT NULL,
    path TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    accessed_at INTEGER NOT NULL,
    bytes BLOB NOT NULL,
    PRIMARY KEY (size, path)
);
CREATE TABLE tmdb_image_ref (
    language TEXT NOT NULL,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    path TEXT NOT NULL,
    PRIMARY KEY (language, kind, id, path)
);
CREATE INDEX tmdb_image_ref_path ON tmdb_image_ref(path);
CREATE INDEX tmdb_image_accessed ON tmdb_image(accessed_at);
CREATE TABLE watch_progress (
    kind TEXT NOT NULL,
    id INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
);
";

/// Failures opening or talking to the local database.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not determine executable directory")]
    NoExeDir(#[source] io::Error),
    #[error("sqlite error")]
    Sqlite(#[from] rusqlite::Error),
    #[error("failed to serialize cache payload")]
    Serialize(#[source] serde_json::Error),
    #[error("failed to parse cache payload")]
    Deserialize(#[source] serde_json::Error),
}

/// Cached JSON plus when it was fetched (unix seconds).
#[derive(Debug, Clone)]
pub struct CacheHit<T> {
    pub value: T,
    pub fetched_at: i64,
}

impl<T> CacheHit<T> {
    #[must_use]
    pub fn is_fresh(&self, ttl: Duration) -> bool {
        age_secs(self.fetched_at) < i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX)
    }
}

/// Local app database (TMDB cache + future watch history).
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Next to the executable: `cinebox.sqlite`.
    ///
    /// # Errors
    ///
    /// Missing executable dir or sqlite errors.
    pub fn system() -> Result<Self, StoreError> {
        let dir = paths::exe_dir().map_err(StoreError::NoExeDir)?;
        Self::open(dir.join("cinebox.sqlite"))
    }

    /// Open (or create) the database at `path`.
    ///
    /// # Errors
    ///
    /// Sqlite setup failures.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        configure(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// In-memory database for tests.
    ///
    /// # Errors
    ///
    /// Sqlite setup failures.
    pub fn memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        configure(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Drop TMDB JSON older than six months, then collect unreferenced images.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub fn maintenance(&self, allowed_sizes: &[String]) -> Result<(), StoreError> {
        self.purge_expired()?;
        self.gc_images(allowed_sizes)
    }

    /// Delete every TMDB cache table. Leaves `watch_progress` alone.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub fn clear_tmdb(&self) -> Result<(), StoreError> {
        let conn = self.lock();
        conn.execute_batch(
            "DELETE FROM tmdb_image_ref;
             DELETE FROM tmdb_image;
             DELETE FROM tmdb_cache;",
        )?;
        Ok(())
    }

    /// Load a cache row if it is younger than six months.
    ///
    /// # Errors
    ///
    /// Sqlite or JSON parse failures.
    pub fn get_json<T: DeserializeOwned>(
        &self,
        language: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<CacheHit<T>>, StoreError> {
        let conn = self.lock();
        let row: Option<(i64, String)> = conn
            .query_row(
                "SELECT fetched_at, payload FROM tmdb_cache
                 WHERE language = ?1 AND kind = ?2 AND id = ?3",
                params![language, kind, id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let Some((fetched_at, payload)) = row else {
            return Ok(None);
        };

        if age_secs(fetched_at) >= i64::try_from(MAX_AGE.as_secs()).unwrap_or(i64::MAX) {
            return Ok(None);
        }

        let value = serde_json::from_str(&payload).map_err(StoreError::Deserialize)?;
        Ok(Some(CacheHit { value, fetched_at }))
    }

    /// Insert or replace a cache row and its image path refs, then run image GC.
    ///
    /// # Errors
    ///
    /// Sqlite or JSON serialize failures.
    pub fn put_json<T: Serialize>(
        &self,
        language: &str,
        kind: &str,
        id: &str,
        value: &T,
        image_paths: &[String],
        allowed_sizes: &[String],
    ) -> Result<(), StoreError> {
        self.put_json_at(
            language,
            kind,
            id,
            value,
            image_paths,
            unix_now(),
            allowed_sizes,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn put_json_at<T: Serialize>(
        &self,
        language: &str,
        kind: &str,
        id: &str,
        value: &T,
        image_paths: &[String],
        fetched_at: i64,
        allowed_sizes: &[String],
    ) -> Result<(), StoreError> {
        let payload = serde_json::to_string(value).map_err(StoreError::Serialize)?;
        {
            let mut conn = self.lock();
            let tx = conn.transaction()?;

            tx.execute(
                "INSERT OR REPLACE INTO tmdb_cache (language, kind, id, fetched_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![language, kind, id, fetched_at, payload],
            )?;

            tx.execute(
                "DELETE FROM tmdb_image_ref WHERE language = ?1 AND kind = ?2 AND id = ?3",
                params![language, kind, id],
            )?;

            for path in image_paths {
                let Some(path) = normalize_tmdb_path(Some(path.as_str())) else {
                    continue;
                };

                tx.execute(
                    "INSERT OR IGNORE INTO tmdb_image_ref (language, kind, id, path)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![language, kind, id, path],
                )?;
            }

            tx.commit()?;
        }

        self.gc_images(allowed_sizes)
    }

    /// Home shelves from disk. `fresh` is true when every remote row is within its TTL.
    ///
    /// # Errors
    ///
    /// Sqlite or JSON parse failures.
    pub fn home_catalog(&self, language: &str) -> Result<Option<(HomeCatalog, bool)>, StoreError> {
        let mut rows = vec![HomeRow::empty(HomeRowId::RecentlyWatched)];
        let mut any = false;
        let mut fresh = true;

        for id in HomeRowId::REMOTE {
            match self.get_json::<HomeRow>(language, KIND_HOME, id.as_key())? {
                None => {
                    fresh = false;
                    rows.push(HomeRow::empty(id));
                }
                Some(hit) => {
                    any = true;
                    if !hit.is_fresh(home_ttl(id)) {
                        fresh = false;
                    }
                    rows.push(hit.value);
                }
            }
        }

        if !any {
            return Ok(None);
        }

        Ok(Some((HomeCatalog { rows }, fresh)))
    }

    /// Read image bytes and bump `accessed_at`.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub fn get_image(&self, size: &str, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(path) = normalize_tmdb_path(Some(path)) else {
            return Ok(None);
        };

        let conn = self.lock();
        let row: Option<(i64, Vec<u8>)> = conn
            .query_row(
                "SELECT fetched_at, bytes FROM tmdb_image WHERE size = ?1 AND path = ?2",
                params![size, path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let Some((fetched_at, bytes)) = row else {
            return Ok(None);
        };

        if age_secs(fetched_at) >= i64::try_from(MAX_AGE.as_secs()).unwrap_or(i64::MAX) {
            return Ok(None);
        }

        conn.execute(
            "UPDATE tmdb_image SET accessed_at = ?1 WHERE size = ?2 AND path = ?3",
            params![unix_now(), size, path],
        )?;

        Ok(Some(bytes))
    }

    /// Store an image blob and enforce the size cap.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub fn put_image(&self, size: &str, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let Some(path) = normalize_tmdb_path(Some(path)) else {
            return Ok(());
        };

        let now = unix_now();
        let conn = self.lock();

        conn.execute(
            "INSERT OR REPLACE INTO tmdb_image (size, path, fetched_at, accessed_at, bytes)
             VALUES (?1, ?2, ?3, ?3, ?4)",
            params![size, path, now, bytes],
        )?;

        evict_over_budget(&conn)?;
        Ok(())
    }

    fn purge_expired(&self) -> Result<(), StoreError> {
        let max_age = i64::try_from(MAX_AGE.as_secs()).unwrap_or(i64::MAX);
        let cutoff = unix_now().saturating_sub(max_age);
        let conn = self.lock();

        conn.execute(
            "DELETE FROM tmdb_cache WHERE fetched_at < ?1",
            params![cutoff],
        )?;

        conn.execute(
            "DELETE FROM tmdb_image_ref WHERE NOT EXISTS (
                SELECT 1 FROM tmdb_cache c
                WHERE c.language = tmdb_image_ref.language
                  AND c.kind = tmdb_image_ref.kind
                  AND c.id = tmdb_image_ref.id
            )",
            [],
        )?;

        conn.execute(
            "DELETE FROM tmdb_image WHERE fetched_at < ?1",
            params![cutoff],
        )?;

        Ok(())
    }

    /// Drop unused sizes, unreferenced paths (after a grace window), then LRU over budget.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub fn gc_images(&self, allowed_sizes: &[String]) -> Result<(), StoreError> {
        let conn = self.lock();

        if !allowed_sizes.is_empty() {
            let placeholders = allowed_sizes
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(", ");

            let sql = format!("DELETE FROM tmdb_image WHERE size NOT IN ({placeholders})");
            let mut stmt = conn.prepare(&sql)?;
            let params = rusqlite::params_from_iter(allowed_sizes.iter());

            stmt.execute(params)?;
        }

        let grace_secs = i64::try_from(IMAGE_GC_GRACE.as_secs()).unwrap_or(0);
        let grace_cutoff = unix_now().saturating_sub(grace_secs);

        conn.execute(
            "DELETE FROM tmdb_image
             WHERE path NOT IN (SELECT path FROM tmdb_image_ref)
               AND accessed_at < ?1",
            params![grace_cutoff],
        )?;

        evict_over_budget(&conn)?;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

fn configure(conn: &Connection) -> Result<(), StoreError> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(Duration::from_secs(5))?;

    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", 1)?;
    }

    Ok(())
}

fn evict_over_budget(conn: &Connection) -> Result<(), rusqlite::Error> {
    loop {
        let total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(bytes)), 0) FROM tmdb_image",
            [],
            |row| row.get(0),
        )?;

        if total <= IMAGE_BUDGET_BYTES {
            break;
        }

        let deleted = conn.execute(
            "DELETE FROM tmdb_image WHERE rowid = (
                SELECT rowid FROM tmdb_image ORDER BY accessed_at ASC LIMIT 1
            )",
            [],
        )?;

        if deleted == 0 {
            break;
        }
    }
    Ok(())
}

fn unix_now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    )
    .unwrap_or(0)
}

fn age_secs(fetched_at: i64) -> i64 {
    unix_now().saturating_sub(fetched_at)
}

/// Fresh TTL for a home shelf.
#[must_use]
pub const fn home_ttl(id: HomeRowId) -> Duration {
    match id {
        HomeRowId::NowPlaying | HomeRowId::TrendingDay => HOME_FAST_TTL,
        _ => HOME_SLOW_TTL,
    }
}

/// Fresh TTL for a media card.
#[must_use]
pub fn media_ttl(details: &MediaDetails) -> Duration {
    let has_release_date = details.released.as_ref().is_some_and(|s| !s.is_empty());

    if details.kind == MediaKind::Movie && has_release_date {
        return DETAILS_STABLE_TTL;
    }

    DETAILS_TTL
}

/// Cache id for a movie/TV card.
#[must_use]
pub fn media_cache_id(kind: MediaKind, id: TmdbId) -> String {
    let kind = match kind {
        MediaKind::Movie => "movie",
        MediaKind::Tv => "tv",
        MediaKind::Person => "person",
    };

    format!("{}:{}", kind, id.get())
}

/// Cache id for a person page.
#[must_use]
pub fn person_cache_id(id: TmdbId) -> String {
    id.get().to_string()
}

/// Cache id for one TV season.
#[must_use]
pub fn season_cache_id(tv_id: TmdbId, season: u32) -> String {
    format!("{}:{season}", tv_id.get())
}

/// Language column value (`""` when unset).
#[must_use]
pub fn language_key(language: Option<&str>) -> &str {
    language.filter(|s| !s.is_empty()).unwrap_or("")
}

/// Image size tokens that must stay after GC.
#[must_use]
pub fn allowed_image_sizes(poster: PosterSize) -> Vec<String> {
    vec![
        poster.tmdb_path().to_owned(),
        String::from("w185"),
        String::from("w1280"),
        String::from("w1280~soft"),
        String::from("w300"),
    ]
}

/// Size column for an image URL, including the soften suffix.
#[must_use]
pub fn image_size_key(size: &str, soften: bool) -> String {
    if soften {
        return format!("{size}~soft");
    }

    size.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CatalogItem;

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

    #[test]
    fn json_roundtrip_is_language_scoped() -> Result<(), StoreError> {
        let store = Store::memory()?;
        let row = sample_row("/a.jpg");

        store.put_json(
            "en-US",
            KIND_HOME,
            row.id.as_key(),
            &row,
            &row.image_paths(),
            &sizes(),
        )?;

        let got = store.get_json::<HomeRow>("en-US", KIND_HOME, row.id.as_key())?;
        assert!(got.is_some());

        if let Some(hit) = got {
            assert_eq!(hit.value.items[0].title, "Dune");
        }

        assert!(
            store
                .get_json::<HomeRow>("ru-RU", KIND_HOME, row.id.as_key())?
                .is_none()
        );

        Ok(())
    }

    #[test]
    fn expired_json_is_hidden() -> Result<(), StoreError> {
        let store = Store::memory()?;
        let row = sample_row("/a.jpg");
        let old = unix_now() - i64::try_from(MAX_AGE.as_secs()).unwrap_or(0) - 10;

        store.put_json_at(
            "",
            KIND_HOME,
            row.id.as_key(),
            &row,
            &row.image_paths(),
            old,
            &sizes(),
        )?;

        assert!(
            store
                .get_json::<HomeRow>("", KIND_HOME, row.id.as_key())?
                .is_none()
        );

        Ok(())
    }

    #[test]
    fn unreferenced_image_is_collected() -> Result<(), StoreError> {
        let store = Store::memory()?;
        store.put_image("w500", "/old.jpg", b"old")?;
        store.put_image("w500", "/new.jpg", b"new")?;

        {
            let conn = store.lock();
            conn.execute(
                "UPDATE tmdb_image SET accessed_at = 1 WHERE path = '/old.jpg'",
                [],
            )?;
        }

        let row = sample_row("/new.jpg");
        store.put_json(
            "",
            KIND_HOME,
            row.id.as_key(),
            &row,
            &row.image_paths(),
            &sizes(),
        )?;

        assert!(store.get_image("w500", "/old.jpg")?.is_none());
        assert_eq!(store.get_image("w500", "/new.jpg")?, Some(b"new".to_vec()));

        Ok(())
    }

    #[test]
    fn clear_tmdb_keeps_watch_progress() -> Result<(), StoreError> {
        let store = Store::memory()?;
        let row = sample_row("/a.jpg");

        store.put_json(
            "",
            KIND_HOME,
            row.id.as_key(),
            &row,
            &row.image_paths(),
            &sizes(),
        )?;

        store.put_image("w500", "/a.jpg", b"img")?;
        {
            let conn = store.lock();
            conn.execute(
                "INSERT INTO watch_progress (kind, id) VALUES ('movie', 7)",
                [],
            )?;
        }

        store.clear_tmdb()?;
        assert!(
            store
                .get_json::<HomeRow>("", KIND_HOME, row.id.as_key())?
                .is_none()
        );

        assert!(store.get_image("w500", "/a.jpg")?.is_none());
        let kept: i64 = {
            let conn = store.lock();
            conn.query_row(
                "SELECT COUNT(*) FROM watch_progress WHERE kind = 'movie' AND id = 7",
                [],
                |row| row.get(0),
            )?
        };

        assert_eq!(kept, 1);
        Ok(())
    }

    #[test]
    fn home_catalog_reports_freshness() -> Result<(), StoreError> {
        let store = Store::memory()?;
        assert!(store.home_catalog("")?.is_none());

        for id in HomeRowId::REMOTE {
            let row = HomeRow {
                id,
                items: vec![sample_item("/a.jpg")],
                error: None,
            };
            store.put_json(
                "",
                KIND_HOME,
                id.as_key(),
                &row,
                &row.image_paths(),
                &sizes(),
            )?;
        }

        let got = store.home_catalog("")?;
        assert!(got.is_some());

        if let Some((catalog, fresh)) = got {
            assert!(fresh);
            assert_eq!(catalog.rows.len(), HomeRowId::ALL.len());
        }

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
}
