//! Last catalog searches typed in the header.

use super::types::{SEARCH_HISTORY_LIMIT, unix_now};
use super::{Store, StoreError};

impl Store {
    /// Newest search strings first, up to `limit`.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn recent_searches(&self, limit: usize) -> Result<Vec<String>, StoreError> {
        let default_limit = i64::try_from(SEARCH_HISTORY_LIMIT).unwrap_or(10);
        let limit = i64::try_from(limit).unwrap_or(default_limit);
        let rows = sqlx::query!(
            r#"
            SELECT query
            FROM search_history
            ORDER BY searched_at DESC, rowid DESC
            LIMIT ?
            "#,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.query).collect())
    }

    /// Remember `query` as the most recent search and keep at most
    /// [`SEARCH_HISTORY_LIMIT`] rows. Whitespace-only strings are ignored.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn record_search(&self, query: &str) -> Result<(), StoreError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(());
        }

        let now = unix_now();
        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO search_history (query, searched_at)
            VALUES (?, ?)
            "#,
            query,
            now
        )
        .execute(&self.pool)
        .await?;

        let limit = i64::try_from(SEARCH_HISTORY_LIMIT).unwrap_or(10);
        sqlx::query!(
            r#"
            DELETE FROM search_history
            WHERE query NOT IN (
                SELECT query FROM search_history
                ORDER BY searched_at DESC, rowid DESC
                LIMIT ?
            )
            "#,
            limit
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
