//! User lists: watch status and the liked flag, with a denormalized poster card.

use crate::catalog::CatalogItem;
use crate::ids::TmdbId;
use crate::library::{LibraryEntry, LibraryMark, ListStatus};
use crate::section::Section;

use super::types::{media_kind_from_key, media_kind_key, unix_now};
use super::{Store, StoreError};

/// Card columns shared by every library upsert.
struct Snapshot<'a> {
    kind: &'static str,
    id: i64,
    section: &'static str,
    title: &'a str,
    poster_path: Option<&'a str>,
    year: Option<i64>,
    vote: Option<f64>,
    now: i64,
}

impl<'a> Snapshot<'a> {
    fn new(item: &'a CatalogItem, section: Section) -> Self {
        Self {
            kind: media_kind_key(item.kind),
            id: i64::from(item.id.get()),
            section: section.as_key(),
            title: &item.title,
            poster_path: item.poster_path.as_deref(),
            year: item.year.map(i64::from),
            vote: item.vote.map(f64::from),
            now: unix_now(),
        }
    }
}

impl Store {
    /// Every library row, most recently changed first.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn library_entries(&self) -> Result<Vec<LibraryEntry>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT kind, id, status, liked, section, title, poster_path, year, vote
            FROM library
            ORDER BY updated_at DESC, rowid DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(kind) = media_kind_from_key(&row.kind) else {
                continue;
            };

            let Ok(id) = u32::try_from(row.id) else {
                continue;
            };

            let section = Section::from_key(&row.section).unwrap_or(Section::fallback(kind));
            let status = row.status.as_deref().and_then(ListStatus::from_key);

            entries.push(LibraryEntry {
                item: CatalogItem {
                    id: TmdbId::new(id),
                    kind,
                    title: row.title,
                    year: row.year.and_then(|year| u16::try_from(year).ok()),
                    vote: row.vote.map(|vote| vote as f32),
                    poster_path: row.poster_path,
                },
                section,
                mark: LibraryMark {
                    status,
                    liked: row.liked != 0,
                },
            });
        }

        Ok(entries)
    }

    /// Set or clear the watch status of `item`. The liked flag is kept.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn set_library_status(
        &self,
        item: &CatalogItem,
        section: Section,
        status: Option<ListStatus>,
    ) -> Result<(), StoreError> {
        let card = Snapshot::new(item, section);
        let status = status.map(ListStatus::as_key);
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r#"
            INSERT INTO library
                (kind, id, status, liked, section, title, poster_path, year, vote,
                 added_at, updated_at)
            VALUES (?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (kind, id) DO UPDATE SET
                status = excluded.status,
                section = excluded.section,
                title = excluded.title,
                poster_path = excluded.poster_path,
                year = excluded.year,
                vote = excluded.vote,
                updated_at = excluded.updated_at
            "#,
            card.kind,
            card.id,
            status,
            card.section,
            card.title,
            card.poster_path,
            card.year,
            card.vote,
            card.now,
            card.now
        )
        .execute(&mut *tx)
        .await?;

        prune_empty(&mut tx, card.kind, card.id).await?;
        tx.commit().await?;

        Ok(())
    }

    /// Set or clear the liked flag of `item`. The watch status is kept.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn set_library_liked(
        &self,
        item: &CatalogItem,
        section: Section,
        liked: bool,
    ) -> Result<(), StoreError> {
        let card = Snapshot::new(item, section);
        let liked = i64::from(liked);
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r#"
            INSERT INTO library
                (kind, id, status, liked, section, title, poster_path, year, vote,
                 added_at, updated_at)
            VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (kind, id) DO UPDATE SET
                liked = excluded.liked,
                section = excluded.section,
                title = excluded.title,
                poster_path = excluded.poster_path,
                year = excluded.year,
                vote = excluded.vote,
                updated_at = excluded.updated_at
            "#,
            card.kind,
            card.id,
            liked,
            card.section,
            card.title,
            card.poster_path,
            card.year,
            card.vote,
            card.now,
            card.now
        )
        .execute(&mut *tx)
        .await?;

        prune_empty(&mut tx, card.kind, card.id).await?;
        tx.commit().await?;

        Ok(())
    }

    /// Put `item` into [`ListStatus::Watching`] unless the user already chose a status.
    ///
    /// # Errors
    ///
    /// Sqlite failures.
    pub async fn mark_watching_if_unset(
        &self,
        item: &CatalogItem,
        section: Section,
    ) -> Result<(), StoreError> {
        let card = Snapshot::new(item, section);
        let watching = ListStatus::Watching.as_key();

        sqlx::query!(
            r#"
            INSERT INTO library
                (kind, id, status, liked, section, title, poster_path, year, vote,
                 added_at, updated_at)
            VALUES (?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (kind, id) DO UPDATE SET
                status = excluded.status,
                updated_at = excluded.updated_at
            WHERE library.status IS NULL
            "#,
            card.kind,
            card.id,
            watching,
            card.section,
            card.title,
            card.poster_path,
            card.year,
            card.vote,
            card.now,
            card.now
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

async fn prune_empty(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    kind: &str,
    id: i64,
) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        DELETE FROM library
        WHERE kind = ? AND id = ? AND status IS NULL AND liked = 0
        "#,
        kind,
        id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MediaKind;

    fn show(id: u32) -> CatalogItem {
        CatalogItem {
            id: TmdbId::new(id),
            kind: MediaKind::Tv,
            title: format!("Show {id}"),
            year: Some(2020),
            vote: Some(7.5),
            poster_path: Some(String::from("/s.jpg")),
        }
    }

    fn mark_of(entries: &[LibraryEntry], id: u32) -> Option<LibraryMark> {
        entries
            .iter()
            .find(|entry| entry.item.id == TmdbId::new(id))
            .map(|entry| entry.mark)
    }

    #[tokio::test]
    async fn new_status_replaces_previous_status() -> Result<(), StoreError> {
        let store = Store::memory().await?;

        store
            .set_library_status(&show(1), Section::Tv, Some(ListStatus::Planned))
            .await?;
        store
            .set_library_status(&show(1), Section::Tv, Some(ListStatus::Completed))
            .await?;

        let entries = store.library_entries().await?;
        assert_eq!(
            mark_of(&entries, 1).and_then(|mark| mark.status),
            Some(ListStatus::Completed)
        );

        Ok(())
    }

    #[tokio::test]
    async fn clearing_status_keeps_liked_row() -> Result<(), StoreError> {
        let store = Store::memory().await?;

        store.set_library_liked(&show(1), Section::Tv, true).await?;
        store
            .set_library_status(&show(1), Section::Tv, Some(ListStatus::Dropped))
            .await?;
        store.set_library_status(&show(1), Section::Tv, None).await?;

        let entries = store.library_entries().await?;
        assert_eq!(
            mark_of(&entries, 1),
            Some(LibraryMark {
                status: None,
                liked: true
            })
        );

        Ok(())
    }

    #[tokio::test]
    async fn row_is_removed_when_nothing_is_set() -> Result<(), StoreError> {
        let store = Store::memory().await?;

        store
            .set_library_status(&show(1), Section::Tv, Some(ListStatus::Watching))
            .await?;
        store.set_library_status(&show(1), Section::Tv, None).await?;

        assert!(store.library_entries().await?.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn mark_watching_does_not_override_chosen_status() -> Result<(), StoreError> {
        let store = Store::memory().await?;

        store
            .set_library_status(&show(1), Section::Tv, Some(ListStatus::Planned))
            .await?;
        store.mark_watching_if_unset(&show(1), Section::Tv).await?;

        let entries = store.library_entries().await?;
        assert_eq!(
            mark_of(&entries, 1).and_then(|mark| mark.status),
            Some(ListStatus::Planned)
        );

        Ok(())
    }

    #[tokio::test]
    async fn mark_watching_fills_liked_only_row() -> Result<(), StoreError> {
        let store = Store::memory().await?;

        store.set_library_liked(&show(1), Section::Tv, true).await?;
        store.mark_watching_if_unset(&show(1), Section::Tv).await?;

        let entries = store.library_entries().await?;
        assert_eq!(
            mark_of(&entries, 1),
            Some(LibraryMark {
                status: Some(ListStatus::Watching),
                liked: true
            })
        );

        Ok(())
    }
}
