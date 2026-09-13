//! Per-title skip-segment choices (`skip_segment_choice` table).
//!
//! `armed = true` after the user pressed Skip (or auto-skip fired) for a
//! given `(kind, tmdb_id, segment_type)`.  It resets to `false` when the user
//! presses Cancel during a countdown.

use std::collections::HashMap;

use super::types::unix_now;
use super::{Store, StoreError};

impl Store {
    /// Load all armed flags for a title in one query.
    ///
    /// Returns a map from segment-type string (`"intro"`, `"recap"`, …) to
    /// `armed` boolean.  Missing rows default to `false` at the call site.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn get_skip_choices(
        &self,
        kind: &str,
        tmdb_id: i64,
    ) -> Result<HashMap<String, bool>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT segment_type, armed
            FROM skip_segment_choice
            WHERE kind = ? AND tmdb_id = ?
            "#,
            kind,
            tmdb_id,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = HashMap::new();

        for row in rows {
            out.insert(row.segment_type, row.armed != 0);
        }

        Ok(out)
    }

    /// Insert or replace the armed flag for one segment type.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn set_skip_armed(
        &self,
        kind: &str,
        tmdb_id: i64,
        segment_type: &str,
        armed: bool,
    ) -> Result<(), StoreError> {
        let armed_int: i64 = i64::from(armed);
        let now = unix_now();

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO skip_segment_choice
                (kind, tmdb_id, segment_type, armed, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
            kind,
            tmdb_id,
            segment_type,
            armed_int,
            now,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
