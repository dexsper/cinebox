//! What this device knows about the torrents on the TorrServer it syncs with.

use crate::ids::{MediaKind, TmdbId};

use super::types::{MATCH_RETRY, ServerMatch, age_secs, media_kind_from_key, media_kind_key, unix_now};
use super::{Store, StoreError};

impl Store {
    /// Ties the sync tables to the server at `url`. A different server than last time empties them, so that
    /// one server's torrents and positions never apply to another. `true` when they were emptied.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn bind_torrserver(&self, url: &str) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;

        let origin = sqlx::query_scalar!("SELECT url FROM torrserver_origin WHERE id = 1");
        let known = origin.fetch_optional(&mut *tx).await?;

        if known.as_deref() == Some(url) {
            return Ok(false);
        }

        sqlx::query!("DELETE FROM torrserver_match").execute(&mut *tx).await?;
        sqlx::query!("DELETE FROM torrserver_viewed_seen").execute(&mut *tx).await?;
        sqlx::query!("INSERT OR REPLACE INTO torrserver_origin (id, url) VALUES (1, ?)", url).execute(&mut *tx).await?;

        tx.commit().await?;

        Ok(known.is_some())
    }

    /// The TMDB title found for a torrent before.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn torrserver_match(&self, hash: &str) -> Result<ServerMatch, StoreError> {
        let row = sqlx::query!("SELECT kind, id, checked_at FROM torrserver_match WHERE hash = ?", hash)
            .fetch_optional(&self.pool)
            .await?;

        let Some(row) = row else { return Ok(ServerMatch::Unknown) };

        let kind = row.kind.as_deref().and_then(media_kind_from_key);
        let id = row.id.and_then(|id| u32::try_from(id).ok());
        if let (Some(kind), Some(id)) = (kind, id) {
            return Ok(ServerMatch::Found(kind, TmdbId::new(id)));
        }

        let retry_after = i64::try_from(MATCH_RETRY.as_secs()).unwrap_or(i64::MAX);
        if age_secs(row.checked_at) >= retry_after {
            return Ok(ServerMatch::Unknown);
        }

        Ok(ServerMatch::NotFound)
    }

    /// Remembers the TMDB title of a torrent, or with `None` that it has none.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn put_torrserver_match(&self, hash: &str, title: Option<(MediaKind, TmdbId)>) -> Result<(), StoreError> {
        let kind = title.map(|(kind, _)| media_kind_key(kind));
        let id = title.map(|(_, id)| i64::from(id.get()));
        let now = unix_now();

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO torrserver_match (hash, kind, id, checked_at)
            VALUES (?, ?, ?, ?)
            "#,
            hash,
            kind,
            id,
            now
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Hashes of the server torrents found to be this title.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn torrserver_hashes(&self, kind: MediaKind, id: TmdbId) -> Result<Vec<String>, StoreError> {
        let kind = media_kind_key(kind);
        let id = i64::from(id.get());
        let rows = sqlx::query!("SELECT hash FROM torrserver_match WHERE kind = ? AND id = ?", kind, id)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|row| row.hash).collect())
    }

    /// The last `/viewed` position of each file of a torrent this device read or sent, as
    /// `(file_index, timecode)` pairs.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn viewed_seen(&self, hash: &str) -> Result<Vec<(i32, f64)>, StoreError> {
        let rows = sqlx::query!("SELECT file_index, timecode FROM torrserver_viewed_seen WHERE hash = ?", hash)
            .fetch_all(&self.pool)
            .await?;

        let pairs = rows.into_iter().filter_map(|row| Some((i32::try_from(row.file_index).ok()?, row.timecode)));

        Ok(pairs.collect())
    }

    /// Records the `/viewed` position this device just read or sent for one file.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn put_viewed_seen(&self, hash: &str, file_index: i32, timecode: f64) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO torrserver_viewed_seen (hash, file_index, timecode)
            VALUES (?, ?, ?)
            "#,
            hash,
            file_index,
            timecode
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_match_is_remembered_by_hash_and_title() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        let dune = (MediaKind::Movie, TmdbId::new(438_631));

        store.put_torrserver_match("aa", Some(dune)).await?;

        assert_eq!(store.torrserver_match("aa").await?, ServerMatch::Found(dune.0, dune.1));
        assert_eq!(store.torrserver_match("bb").await?, ServerMatch::Unknown);
        assert_eq!(store.torrserver_hashes(dune.0, dune.1).await?, vec![String::from("aa")]);

        Ok(())
    }

    #[tokio::test]
    async fn a_miss_is_retried_after_a_day() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        store.put_torrserver_match("aa", None).await?;

        assert_eq!(store.torrserver_match("aa").await?, ServerMatch::NotFound);

        sqlx::query("UPDATE torrserver_match SET checked_at = 0").execute(&store.pool).await?;

        assert_eq!(store.torrserver_match("aa").await?, ServerMatch::Unknown);

        Ok(())
    }

    #[tokio::test]
    async fn another_server_empties_the_sync_tables() -> Result<(), StoreError> {
        let store = Store::memory().await?;
        assert!(!store.bind_torrserver("http://home:8090").await?);

        store.put_torrserver_match("aa", None).await?;
        store.put_viewed_seen("aa", 1, 300.0).await?;

        assert!(!store.bind_torrserver("http://home:8090").await?);
        assert_eq!(store.viewed_seen("aa").await?, vec![(1, 300.0)]);

        assert!(store.bind_torrserver("http://public:8090").await?);
        assert_eq!(store.viewed_seen("aa").await?, Vec::new());
        assert_eq!(store.torrserver_match("aa").await?, ServerMatch::Unknown);

        Ok(())
    }
}
