//! Per-torrent playback preferences.

use super::types::{TorrentPlaybackPrefs, unix_now};
use super::{Store, StoreError};

impl Store {
    /// Playback prefs saved for a torrent hash.
    ///
    /// # Errors
    ///
    /// Sqlite failures. A payload that no longer parses reads as `None`.
    pub async fn get_torrent_prefs(
        &self,
        hash: &str,
    ) -> Result<Option<TorrentPlaybackPrefs>, StoreError> {
        let row = sqlx::query!(
            "SELECT payload FROM torrent_playback_prefs WHERE hash = ?",
            hash
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(serde_json::from_str(&row.payload).ok())
    }

    /// Insert or replace playback prefs for a torrent hash.
    ///
    /// # Errors
    ///
    /// Sqlite or JSON serialize failures.
    pub async fn put_torrent_prefs(
        &self,
        hash: &str,
        prefs: &TorrentPlaybackPrefs,
    ) -> Result<(), StoreError> {
        let payload = serde_json::to_string(prefs).map_err(StoreError::Serialize)?;
        let now = unix_now();

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO torrent_playback_prefs (hash, payload, updated_at)
            VALUES (?, ?, ?)
            "#,
            hash,
            payload,
            now
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
