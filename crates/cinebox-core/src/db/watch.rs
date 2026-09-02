//! Watch timeline and recently-watched history.

use crate::catalog::{CatalogItem, HomeRow, HomeRowId};
use crate::ids::{MediaKind, TmdbId};

use super::types::{
    RECENT_ROW_LIMIT, WatchHistoryEntry, episode_key, media_kind_from_key, media_kind_key, unix_now,
};
use super::{Store, StoreError};

impl Store {
    /// Record playback position for one episode (or a movie), keyed by
    /// content identity — never by torrent hash.
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
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn upsert_watch_history(&self, entry: &WatchHistoryEntry) -> Result<(), StoreError> {
        let kind = media_kind_key(entry.kind);
        let id = i64::from(entry.id.get());
        let year = entry.year.map(i64::from);
        let vote = entry.vote.map(f64::from);
        let season = entry.season.map(i64::from);
        let episode = entry.episode.map(i64::from);
        let poster_path = entry.poster_path.as_deref();
        let episode_title = entry.episode_title.as_deref();
        let last_hash = entry.last_hash.as_deref();
        let now = unix_now();

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO watch_history
                (kind, id, title, poster_path, year, vote, season, episode,
                 episode_title, time, duration, last_hash, updated_at)
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
            last_hash,
            now
        )
        .execute(&self.pool)
        .await?;

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

    /// Torrent hash last used for this media, if any.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn watch_history_last_hash(
        &self,
        kind: MediaKind,
        id: TmdbId,
    ) -> Result<Option<String>, StoreError> {
        let kind = media_kind_key(kind);
        let id = i64::from(id.get());
        let row = sqlx::query!(
            "SELECT last_hash FROM watch_history WHERE kind = ? AND id = ?",
            kind,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|row| row.last_hash))
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
