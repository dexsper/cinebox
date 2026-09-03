//! Watch timeline and recently-watched history.

use crate::catalog::{CatalogItem, HomeRow, HomeRowId};
use crate::ids::{MediaKind, TmdbId};

use super::types::{
    RECENT_RELEASE_LIMIT, RECENT_ROW_LIMIT, WatchHistoryEntry, episode_key, media_kind_from_key,
    media_kind_key, unix_now,
};
use super::{Store, StoreError};

impl Store {
    /// Record playback position for one episode (or a movie), keyed by
    /// content identity never by torrent hash.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn upsert_watch_timeline(
        &self,
        kind: MediaKind,
        id: TmdbId,
        season: Option<u32>,
        episode: Option<u32>,
        time: f64,
        duration: f64,
    ) -> Result<(), StoreError> {
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

    /// Saved `(time, duration)` for one episode (or a movie), if any.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn get_watch_timeline(
        &self,
        kind: MediaKind,
        id: TmdbId,
        season: Option<u32>,
        episode: Option<u32>,
    ) -> Result<Option<(f64, f64)>, StoreError> {
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
    pub async fn upsert_watch_history(
        &self,
        entry: &WatchHistoryEntry,
        hash: Option<&str>,
    ) -> Result<(), StoreError> {
        let kind = media_kind_key(entry.kind);
        let id = i64::from(entry.id.get());
        let year = entry.year.map(i64::from);
        let vote = entry.vote.map(f64::from);
        let season = entry.season.map(i64::from);
        let episode = entry.episode.map(i64::from);
        let poster_path = entry.poster_path.as_deref();
        let episode_title = entry.episode_title.as_deref();
        let now = unix_now();

        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO watch_history
                (kind, id, title, poster_path, year, vote, season, episode,
                 episode_title, time, duration, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            now
        )
        .execute(&mut *tx)
        .await?;

        touch_watch_release(&mut tx, entry.kind, entry.id, hash).await?;

        tx.commit().await?;

        Ok(())
    }

    /// Home-shelf tiles, most recently watched first.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn recently_watched(&self, limit: usize) -> Result<Vec<CatalogItem>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = sqlx::query!(
            r#"
            SELECT kind, id, title, poster_path, year, vote
            FROM watch_history
            ORDER BY updated_at DESC, rowid DESC
            LIMIT ?
            "#,
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
            let vote = row.vote.map(|vote| vote as f32);

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

    /// Every media the user ever started playing, for the poster badge set.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn watched_keys(&self) -> Result<Vec<(MediaKind, TmdbId)>, StoreError> {
        let rows = sqlx::query!("SELECT kind, id FROM watch_history")
            .fetch_all(&self.pool)
            .await?;

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
    pub async fn watch_release_hashes(
        &self,
        kind: MediaKind,
        id: TmdbId,
    ) -> Result<Vec<String>, StoreError> {
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
            items: self.recently_watched(RECENT_ROW_LIMIT).await?,
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
