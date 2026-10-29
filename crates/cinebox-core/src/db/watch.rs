//! Watch timeline and recently-watched history.

use crate::catalog::{CatalogItem, HomeRow, HomeRowId};
use crate::ids::{MediaKind, TmdbId};
use crate::section::Section;

use super::types::{
    RECENT_RELEASE_LIMIT, RECENT_ROW_LIMIT, WatchHistoryEntry, WatchKey, episode_key, media_kind_from_key,
    media_kind_key, unix_now, vote_from_real,
};
use super::{Store, StoreError};

impl Store {
    /// Record playback position for one episode (or a movie), keyed by
    /// content identity never by torrent hash.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn upsert_watch_timeline(&self, key: WatchKey, time: f64, duration: f64) -> Result<(), StoreError> {
        let WatchKey { kind, id, season, episode } = key;
        let kind = media_kind_key(kind);
        let id = i64::from(id.get());
        let season = episode_key(season);
        let episode = episode_key(episode);
        let now = unix_now();

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO watch_timeline
                (kind, id, season, episode, time, duration, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            kind,
            id,
            season,
            episode,
            time,
            duration,
            now
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Record a position another client left on the server. Unlike [`Self::upsert_watch_timeline`], the
    /// duration this device knows is kept, since the server has none.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn import_watch_timeline(&self, key: WatchKey, time: f64) -> Result<(), StoreError> {
        let WatchKey { kind, id, season, episode } = key;
        let kind = media_kind_key(kind);
        let id = i64::from(id.get());
        let season = episode_key(season);
        let episode = episode_key(episode);
        let now = unix_now();

        sqlx::query!(
            r#"
            INSERT INTO watch_timeline (kind, id, season, episode, time, duration, updated_at)
            VALUES (?, ?, ?, ?, ?, 0, ?)
            ON CONFLICT (kind, id, season, episode) DO UPDATE SET
                time = excluded.time,
                updated_at = excluded.updated_at
            "#,
            kind,
            id,
            season,
            episode,
            time,
            now
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Saved `(time, duration)` for one episode (or a movie), if any.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn get_watch_timeline(&self, key: WatchKey) -> Result<Option<(f64, f64)>, StoreError> {
        let WatchKey { kind, id, season, episode } = key;
        let kind = media_kind_key(kind);
        let id = i64::from(id.get());
        let season = episode_key(season);
        let episode = episode_key(episode);

        let row = sqlx::query!(
            r#"
            SELECT time, duration
            FROM watch_timeline
            WHERE kind = ? AND id = ? AND season = ? AND episode = ?
            "#,
            kind,
            id,
            season,
            episode
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| (row.time, row.duration)))
    }

    /// Insert or replace the one history row for a movie/show.
    ///
    /// `hash` is the torrent just played; kept in `watch_release` (up to 3).
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn upsert_watch_history(&self, entry: &WatchHistoryEntry, hash: Option<&str>) -> Result<(), StoreError> {
        let kind = media_kind_key(entry.kind);
        let id = i64::from(entry.id.get());
        let year = entry.year.map(i64::from);
        let vote = entry.vote.map(f64::from);
        let season = entry.season.map(i64::from);
        let episode = entry.episode.map(i64::from);
        let poster_path = entry.poster_path.as_deref();
        let episode_title = entry.episode_title.as_deref();
        let section = entry.section.as_key();
        let now = unix_now();

        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO watch_history
                (kind, id, title, poster_path, year, vote, season, episode,
                 episode_title, time, duration, updated_at, section)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            kind,
            id,
            entry.title,
            poster_path,
            year,
            vote,
            season,
            episode,
            episode_title,
            entry.time,
            entry.duration,
            now,
            section
        )
        .execute(&mut *tx)
        .await?;

        touch_watch_release(&mut tx, entry.kind, entry.id, hash).await?;

        tx.commit().await?;

        Ok(())
    }

    /// Record a title another client played or added on the server, as of `updated_at` (unix seconds).
    ///
    /// Written only when newer than the local row, so local playback since then is never overwritten. An
    /// entry with `time == 0` (added, not started) moves the title up but keeps the local episode and
    /// position, and a zero duration keeps the local one. `hash` joins the title's recent releases.
    /// `true` when the row was written.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn import_watch_history(
        &self,
        entry: &WatchHistoryEntry,
        hash: &str,
        updated_at: i64,
    ) -> Result<bool, StoreError> {
        let kind = media_kind_key(entry.kind);
        let id = i64::from(entry.id.get());
        let year = entry.year.map(i64::from);
        let vote = entry.vote.map(f64::from);
        let season = entry.season.map(i64::from);
        let episode = entry.episode.map(i64::from);
        let poster_path = entry.poster_path.as_deref();
        let episode_title = entry.episode_title.as_deref();
        let section = entry.section.as_key();

        let mut tx = self.pool.begin().await?;

        let inserted = sqlx::query!(
            r#"
            INSERT INTO watch_history
                (kind, id, title, poster_path, year, vote, season, episode,
                 episode_title, time, duration, updated_at, section)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (kind, id) DO UPDATE SET
                title = excluded.title,
                poster_path = excluded.poster_path,
                year = excluded.year,
                vote = excluded.vote,
                section = excluded.section,
                season = CASE WHEN excluded.time > 0 THEN excluded.season ELSE watch_history.season END,
                episode = CASE WHEN excluded.time > 0 THEN excluded.episode ELSE watch_history.episode END,
                episode_title =
                    CASE WHEN excluded.time > 0 THEN excluded.episode_title ELSE watch_history.episode_title END,
                time = CASE WHEN excluded.time > 0 THEN excluded.time ELSE watch_history.time END,
                duration = CASE WHEN excluded.duration > 0 THEN excluded.duration ELSE watch_history.duration END,
                updated_at = excluded.updated_at
            WHERE excluded.updated_at > watch_history.updated_at
            "#,
            kind,
            id,
            entry.title,
            poster_path,
            year,
            vote,
            season,
            episode,
            episode_title,
            entry.time,
            entry.duration,
            updated_at,
            section
        )
        .execute(&mut *tx)
        .await?;

        let written = inserted.rows_affected() > 0;

        if written {
            touch_watch_release(&mut tx, entry.kind, entry.id, Some(hash)).await?;
        }

        tx.commit().await?;

        Ok(written)
    }

    /// Home-shelf tiles, most recently watched first, optionally limited to one section.
    ///
    /// Rows saved before sections existed are placed by media kind.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn recently_watched(
        &self,
        limit: usize,
        section: Option<Section>,
    ) -> Result<Vec<CatalogItem>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let section = section.map(Section::as_key);
        let rows = sqlx::query!(
            r#"
            SELECT kind, id, title, poster_path, year, vote
            FROM watch_history
            WHERE ?1 IS NULL
               OR COALESCE(section, CASE kind WHEN 'tv' THEN 'tv' ELSE 'movies' END) = ?1
            ORDER BY updated_at DESC, rowid DESC
            LIMIT ?2
            "#,
            section,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        let mut items = Vec::new();
        for row in rows {
            let Some(kind) = media_kind_from_key(&row.kind) else {
                continue;
            };

            let Ok(id) = u32::try_from(row.id) else {
                continue;
            };

            let year = row.year.and_then(|year| u16::try_from(year).ok());
            let vote = row.vote.map(vote_from_real);

            items.push(CatalogItem {
                id: TmdbId::new(id),
                kind,
                title: row.title,
                year,
                vote,
                poster_path: row.poster_path,
            });
        }

        Ok(items)
    }

    /// Every media the user ever started playing, here or on another device, for the poster badge set.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn watched_keys(&self) -> Result<Vec<(MediaKind, TmdbId)>, StoreError> {
        // A title imported from the server but never started has no position and no badge.
        let rows = sqlx::query!("SELECT kind, id FROM watch_history WHERE time > 0").fetch_all(&self.pool).await?;

        let mut keys = Vec::new();
        for row in rows {
            let Some(kind) = media_kind_from_key(&row.kind) else {
                continue;
            };

            let Ok(id) = u32::try_from(row.id) else {
                continue;
            };

            keys.push((kind, TmdbId::new(id)));
        }

        Ok(keys)
    }

    /// Recent torrent hashes played for this media, newest first (capped at 3).
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn watch_release_hashes(&self, kind: MediaKind, id: TmdbId) -> Result<Vec<String>, StoreError> {
        let kind = media_kind_key(kind);
        let id = i64::from(id.get());
        let limit = i64::try_from(RECENT_RELEASE_LIMIT).unwrap_or(3);
        let rows = sqlx::query!(
            r#"
            SELECT hash
            FROM watch_release
            WHERE kind = ? AND id = ?
            ORDER BY updated_at DESC, rowid DESC
            LIMIT ?
            "#,
            kind,
            id,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.hash).collect())
    }

    /// The local "recently watched" home shelf. Always queried live: it is
    /// local data with no TTL.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn recently_watched_row(&self) -> Result<HomeRow, StoreError> {
        Ok(HomeRow {
            id: HomeRowId::RecentlyWatched,
            items: self.recently_watched(RECENT_ROW_LIMIT, None).await?,
            error: None,
        })
    }
}

/// Upsert the just-played hash and prune the per-media list to
/// `RECENT_RELEASE_LIMIT` with a single `DELETE`, inside the caller's
/// transaction.
async fn touch_watch_release(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    kind: MediaKind,
    id: TmdbId,
    hash: Option<&str>,
) -> Result<(), StoreError> {
    let Some(hash) = hash.filter(|hash| !hash.is_empty()) else {
        return Ok(());
    };

    let kind = media_kind_key(kind);
    let id = i64::from(id.get());
    let now = unix_now();

    sqlx::query!(
        r#"
        INSERT OR REPLACE INTO watch_release (kind, id, hash, updated_at)
        VALUES (?, ?, ?, ?)
        "#,
        kind,
        id,
        hash,
        now
    )
    .execute(&mut **tx)
    .await?;

    let limit = i64::try_from(RECENT_RELEASE_LIMIT).unwrap_or(3);

    sqlx::query!(
        r#"
        DELETE FROM watch_release
        WHERE kind = ? AND id = ? AND hash NOT IN (
            SELECT hash FROM watch_release
            WHERE kind = ? AND id = ?
            ORDER BY updated_at DESC, rowid DESC
            LIMIT ?
        )
        "#,
        kind,
        id,
        kind,
        id,
        limit
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}
