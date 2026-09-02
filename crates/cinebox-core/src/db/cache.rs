//! TMDB JSON and image blob cache.

use std::collections::HashSet;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::catalog::normalize_tmdb_path;

use super::types::{IMAGE_BUDGET_BYTES, IMAGE_GC_GRACE, MAX_AGE, age_secs, unix_now};
use super::{CacheHit, Store, StoreError};

impl Store {
    /// Delete every TMDB cache table. Leaves watch history and prefs alone.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn clear_tmdb(&self) -> Result<(), StoreError> {
        sqlx::query!("DELETE FROM tmdb_image_ref")
            .execute(&self.pool)
            .await?;

        sqlx::query!("DELETE FROM tmdb_image")
            .execute(&self.pool)
            .await?;

        sqlx::query!("DELETE FROM tmdb_cache")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Load a cache row if it is younger than six months.
    ///
    /// # Errors
    ///
    /// Sqlite or JSON parse failures.
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        language: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<CacheHit<T>>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT fetched_at, payload
            FROM tmdb_cache
            WHERE language = ? AND kind = ? AND id = ?
            "#,
            language,
            kind,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        if age_secs(row.fetched_at) >= i64::try_from(MAX_AGE.as_secs()).unwrap_or(i64::MAX) {
            return Ok(None);
        }

        let value = serde_json::from_str(&row.payload).map_err(StoreError::Deserialize)?;

        Ok(Some(CacheHit {
            value,
            fetched_at: row.fetched_at,
        }))
    }

    /// Insert or replace a cache row and its image path refs, then run image GC.
    ///
    /// # Errors
    ///
    /// Sqlite or JSON serialize failures.
    pub async fn put_json<T: Serialize>(
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
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn put_json_at<T: Serialize>(
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
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO tmdb_cache (language, kind, id, fetched_at, payload)
            VALUES (?, ?, ?, ?, ?)
            "#,
            language,
            kind,
            id,
            fetched_at,
            payload
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            DELETE FROM tmdb_image_ref
            WHERE language = ? AND kind = ? AND id = ?
            "#,
            language,
            kind,
            id
        )
        .execute(&mut *tx)
        .await?;

        for path in image_paths {
            let Some(path) = normalize_tmdb_path(Some(path.as_str())) else {
                continue;
            };

            sqlx::query!(
                r#"
                INSERT OR IGNORE INTO tmdb_image_ref (language, kind, id, path)
                VALUES (?, ?, ?, ?)
                "#,
                language,
                kind,
                id,
                path
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        self.gc_images(allowed_sizes).await
    }

    /// Read image bytes and bump `accessed_at`.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn get_image(&self, size: &str, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(path) = normalize_tmdb_path(Some(path)) else {
            return Ok(None);
        };

        let row = sqlx::query!(
            r#"
            SELECT fetched_at, bytes
            FROM tmdb_image
            WHERE size = ? AND path = ?
            "#,
            size,
            path
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        if age_secs(row.fetched_at) >= i64::try_from(MAX_AGE.as_secs()).unwrap_or(i64::MAX) {
            return Ok(None);
        }

        let now = unix_now();
        sqlx::query!(
            r#"
            UPDATE tmdb_image
            SET accessed_at = ?
            WHERE size = ? AND path = ?
            "#,
            now,
            size,
            path
        )
        .execute(&self.pool)
        .await?;

        Ok(Some(row.bytes))
    }

    /// Store an image blob and enforce the size cap.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn put_image(&self, size: &str, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let Some(path) = normalize_tmdb_path(Some(path)) else {
            return Ok(());
        };

        let now = unix_now();
        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO tmdb_image (size, path, fetched_at, accessed_at, bytes)
            VALUES (?, ?, ?, ?, ?)
            "#,
            size,
            path,
            now,
            now,
            bytes
        )
        .execute(&self.pool)
        .await?;

        self.evict_over_budget().await
    }

    pub(crate) async fn purge_expired(&self) -> Result<(), StoreError> {
        let max_age = i64::try_from(MAX_AGE.as_secs()).unwrap_or(i64::MAX);
        let cutoff = unix_now().saturating_sub(max_age);

        sqlx::query!("DELETE FROM tmdb_cache WHERE fetched_at < ?", cutoff)
            .execute(&self.pool)
            .await?;

        sqlx::query!(
            r#"
            DELETE FROM tmdb_image_ref
            WHERE NOT EXISTS (
                SELECT 1 FROM tmdb_cache c
                WHERE c.language = tmdb_image_ref.language
                  AND c.kind = tmdb_image_ref.kind
                  AND c.id = tmdb_image_ref.id
            )
            "#
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!("DELETE FROM tmdb_image WHERE fetched_at < ?", cutoff)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Drop unused sizes, unreferenced paths (after a grace window), then LRU over budget.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn gc_images(&self, allowed_sizes: &[String]) -> Result<(), StoreError> {
        if !allowed_sizes.is_empty() {
            let allowed: HashSet<&str> = allowed_sizes.iter().map(String::as_str).collect();
            let sizes = sqlx::query!("SELECT DISTINCT size FROM tmdb_image")
                .fetch_all(&self.pool)
                .await?;

            for row in sizes {
                if allowed.contains(row.size.as_str()) {
                    continue;
                }

                sqlx::query!("DELETE FROM tmdb_image WHERE size = ?", row.size)
                    .execute(&self.pool)
                    .await?;
            }
        }

        let grace_secs = i64::try_from(IMAGE_GC_GRACE.as_secs()).unwrap_or(0);
        let grace_cutoff = unix_now().saturating_sub(grace_secs);

        sqlx::query!(
            r#"
            DELETE FROM tmdb_image
            WHERE path NOT IN (SELECT path FROM tmdb_image_ref)
              AND accessed_at < ?
            "#,
            grace_cutoff
        )
        .execute(&self.pool)
        .await?;

        self.evict_over_budget().await
    }

    async fn evict_over_budget(&self) -> Result<(), StoreError> {
        loop {
            let row =
                sqlx::query!("SELECT COALESCE(SUM(LENGTH(bytes)), 0) AS total FROM tmdb_image")
                    .fetch_one(&self.pool)
                    .await?;

            if row.total <= IMAGE_BUDGET_BYTES {
                break;
            }

            let deleted = sqlx::query!(
                r#"
                DELETE FROM tmdb_image
                WHERE rowid = (
                    SELECT rowid FROM tmdb_image ORDER BY accessed_at ASC LIMIT 1
                )
                "#
            )
            .execute(&self.pool)
            .await?;

            if deleted.rows_affected() == 0 {
                break;
            }
        }

        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn set_image_accessed_at(
        &self,
        path: &str,
        accessed_at: i64,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            "UPDATE tmdb_image SET accessed_at = ? WHERE path = ?",
            accessed_at,
            path
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
