//! TMDB pages, home rows and shelves, read through the local cache.

use std::sync::Arc;

use cinebox_core::{
    HOME_SLOW_TTL, HomeCatalog, HomeRow, HomeRowId, KIND_HOME, KIND_MEDIA, KIND_PERSON,
    MediaDetails, MediaKind, PersonDetails, RECENT_ROW_LIMIT, Section, Store, TmdbId, language_key,
    media_cache_id, person_cache_id, poster_paths,
};
use cinebox_tmdb::{
    DiscoverQuery, SectionRow, ShelfId, ShelfRow, fetch_catalog_page, fetch_discover_page,
    fetch_home, fetch_media, fetch_person, fetch_search_page, fetch_section, section_rows,
};
use tracing::warn;

use super::{JobError, TmdbCtx};

pub async fn load_catalog_page(
    tmdb: TmdbCtx,
    id: ShelfId,
    page: u32,
) -> Result<cinebox_tmdb::CatalogPage, JobError> {
    let language = Some(tmdb.language);
    let page = fetch_catalog_page(&tmdb.api_key, id, page, language, &tmdb.net).await?;

    Ok(page)
}

pub async fn load_discover_page(
    tmdb: TmdbCtx,
    query: DiscoverQuery,
    page: u32,
) -> Result<cinebox_tmdb::CatalogPage, JobError> {
    let language = Some(tmdb.language);
    let page = fetch_discover_page(&tmdb.api_key, &query, page, language, &tmdb.net).await?;

    Ok(page)
}

/// Section hub from disk. `fresh` is true when every remote shelf is within its TTL.
pub async fn cached_section(
    db: Arc<Store>,
    language: String,
    section: Section,
) -> Option<(Vec<ShelfRow>, bool)> {
    let mut rows = Vec::new();
    let mut fresh = true;
    let mut any = false;

    for row in section_rows(section) {
        let id = ShelfId::Section(section, *row);
        if *row == SectionRow::RecentlyWatched {
            rows.push(recent_in_section(&db, section).await);
            continue;
        }

        let cached = db
            .get_json::<ShelfRow>(&language, KIND_HOME, &id.as_key())
            .await
            .ok()
            .flatten();

        // Failures were cached by older versions.
        let loaded = cached.filter(|hit| hit.value.error.is_none());
        let Some(hit) = loaded else {
            fresh = false;
            rows.push(ShelfRow::empty(id));
            continue;
        };

        any = true;
        fresh &= hit.is_fresh(HOME_SLOW_TTL);
        rows.push(hit.value);
    }

    any.then_some((rows, fresh))
}

pub async fn load_section(
    tmdb: TmdbCtx,
    section: Section,
    db: Option<Arc<Store>>,
) -> Result<Vec<ShelfRow>, JobError> {
    let language = Some(tmdb.language);
    let fetched = fetch_section(&tmdb.api_key, section, language, &tmdb.net).await?;

    let Some(db) = db else {
        return Ok(fetched);
    };

    let lang = language_key(language);
    let mut rows = Vec::with_capacity(fetched.len());
    for row in fetched {
        if row.id == ShelfId::Section(section, SectionRow::RecentlyWatched) {
            rows.push(recent_in_section(&db, section).await);
            continue;
        }

        let key = row.id.as_key();
        if row.error.is_some() {
            rows.push(shelf_instead(&db, lang, row).await);
            continue;
        }

        let paths = poster_paths(&row.items);
        if let Err(error) = db.put_json(lang, KIND_HOME, &key, &row, &paths).await {
            warn!(%error, "failed to persist section shelf");
        }

        rows.push(row);
    }

    Ok(rows)
}

/// Like [`home_row_instead`], for a section's shelf.
async fn shelf_instead(db: &Store, lang: &str, failed: ShelfRow) -> ShelfRow {
    let key = failed.id.as_key();
    let error = failed.error.as_deref().unwrap_or_default();
    warn!(shelf = key, error, "section shelf failed to load");

    let cached = db.get_json::<ShelfRow>(lang, KIND_HOME, &key).await;
    match cached {
        Ok(Some(hit)) if hit.value.error.is_none() => hit.value,
        _ => failed,
    }
}

async fn recent_in_section(db: &Store, section: Section) -> ShelfRow {
    let id = ShelfId::Section(section, SectionRow::RecentlyWatched);
    match db.recently_watched(RECENT_ROW_LIMIT, Some(section)).await {
        Ok(items) => ShelfRow {
            id,
            items,
            error: None,
        },
        Err(error) => {
            warn!(%error, "failed to load recently watched");
            ShelfRow::empty(id)
        }
    }
}

pub async fn load_search_page(
    tmdb: TmdbCtx,
    query: String,
    kind: cinebox_tmdb::SearchKind,
    page: u32,
) -> Result<cinebox_tmdb::CatalogPage, JobError> {
    let language = Some(tmdb.language);
    let page = fetch_search_page(&tmdb.api_key, &query, kind, page, language, &tmdb.net).await?;

    Ok(page)
}

pub async fn load_home(tmdb: TmdbCtx, db: Option<Arc<Store>>) -> Result<HomeCatalog, JobError> {
    let language = Some(tmdb.language);
    let fetched = fetch_home(&tmdb.api_key, language, &tmdb.net).await?;

    let Some(db) = db else {
        return Ok(fetched);
    };

    let lang = language_key(language);
    let mut rows = Vec::with_capacity(fetched.rows.len());
    for row in fetched.rows {
        if !row.id.is_remote() && row.id != HomeRowId::RecentlyWatched {
            rows.push(row);
            continue;
        }

        if row.id == HomeRowId::RecentlyWatched {
            match db.recently_watched_row().await {
                Ok(local) => rows.push(local),
                Err(error) => {
                    warn!(%error, "failed to load recently watched");
                    rows.push(HomeRow::empty(HomeRowId::RecentlyWatched));
                }
            }
            continue;
        }

        if row.error.is_some() {
            rows.push(home_row_instead(&db, lang, row).await);
            continue;
        }

        let paths = row.image_paths();
        let saved = db
            .put_json(lang, KIND_HOME, row.id.as_key(), &row, &paths)
            .await;

        if let Err(error) = saved {
            warn!(%error, "failed to persist home row");
        }

        rows.push(row);
    }

    Ok(HomeCatalog { rows })
}

/// A row that failed to load is never cached, or every start would show the
/// failure as fresh without asking TMDB again. The last row that did load
/// stands in for it.
async fn home_row_instead(db: &Store, lang: &str, failed: HomeRow) -> HomeRow {
    let error = failed.error.as_deref().unwrap_or_default();
    warn!(row = failed.id.as_key(), error, "home row failed to load");

    let cached = db
        .get_json::<HomeRow>(lang, KIND_HOME, failed.id.as_key())
        .await;

    match cached {
        Ok(Some(hit)) if hit.value.error.is_none() => hit.value,
        _ => failed,
    }
}

pub async fn load_media(
    tmdb: TmdbCtx,
    kind: MediaKind,
    id: TmdbId,
    db: Option<Arc<Store>>,
) -> Result<Box<MediaDetails>, JobError> {
    let language = Some(tmdb.language);
    let mut details = fetch_media(&tmdb.api_key, kind, id, language, &tmdb.net).await?;

    if let Some(db) = db {
        let lang = language_key(language);
        let cache_id = media_cache_id(kind, id);
        let paths = details.image_paths();
        let saved = db
            .put_json(lang, KIND_MEDIA, &cache_id, &details, &paths)
            .await;

        if let Err(error) = saved {
            warn!(%error, "failed to persist media details");
        }
    }

    details.apply_typography();
    Ok(Box::new(details))
}

pub async fn load_person(
    tmdb: TmdbCtx,
    id: TmdbId,
    db: Option<Arc<Store>>,
) -> Result<Box<PersonDetails>, JobError> {
    let language = Some(tmdb.language);
    let mut details = fetch_person(&tmdb.api_key, id, language, &tmdb.net).await?;

    if let Some(db) = db {
        let lang = language_key(language);
        let cache_id = person_cache_id(id);
        let paths = details.image_paths();
        let saved = db
            .put_json(lang, KIND_PERSON, &cache_id, &details, &paths)
            .await;

        if let Err(error) = saved {
            warn!(%error, "failed to persist person details");
        }
    }

    details.apply_typography();
    Ok(Box::new(details))
}
