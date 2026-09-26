//! Shared services owned by the thin app dispatcher. Screens borrow this.

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, Mutex};

use cinebox_core::{
    LibraryMark, ListStatus, MediaKind, Settings, SettingsStore, Store, StoreError, TmdbId,
    allowed_image_sizes,
};
use cinebox_player::Engine;
use tracing::{error, info, warn};

use crate::images::ImageCache;
use crate::library::Library;
use crate::screens::play::WatchCard;
use crate::toasts::Toasts;
use crate::widgets::poster::TileMarks;

pub struct Services {
    pub settings: Settings,
    pub store: Option<SettingsStore>,
    pub db: Option<Arc<Store>>,
    pub load_error: Option<String>,
    pub save_error: Option<String>,
    pub images: ImageCache,
    pub toasts: Toasts,
    pub engine: Option<Arc<Mutex<Engine>>>,
    pub library: Library,
    watched: HashSet<(MediaKind, TmdbId)>,
    home_needs_refresh: bool,
}

impl Services {
    pub fn boot(engine: Option<Arc<Mutex<Engine>>>) -> Self {
        let (store, settings, load_error) = open_settings_store();
        let db = open_app_db(&settings);
        let images = ImageCache::with_db(db.clone());
        let watched = db
            .as_ref()
            .and_then(|db| db_block_on(db.watched_keys()).ok())
            .unwrap_or_default()
            .into_iter()
            .collect();
        let library = db
            .as_ref()
            .and_then(|db| db_block_on(db.library_entries()).ok())
            .map(Library::from_entries)
            .unwrap_or_default();

        Self {
            settings,
            store,
            db,
            load_error,
            save_error: None,
            images,
            toasts: Toasts::default(),
            engine,
            library,
            watched,
            home_needs_refresh: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_with_db(db: Arc<Store>) -> Self {
        Self {
            settings: Settings::default(),
            store: None,
            db: Some(db.clone()),
            load_error: None,
            save_error: None,
            images: ImageCache::with_db(Some(db)),
            toasts: Toasts::default(),
            engine: None,
            library: Library::default(),
            watched: HashSet::new(),
            home_needs_refresh: false,
        }
    }

    #[must_use]
    pub fn is_watched(&self, kind: MediaKind, id: TmdbId) -> bool {
        self.watched.contains(&(kind, id))
    }

    pub fn mark_watched(&mut self, kind: MediaKind, id: TmdbId) {
        self.watched.insert((kind, id));
        self.home_needs_refresh = true;
    }

    #[must_use]
    pub fn library_mark(&self, kind: MediaKind, id: TmdbId) -> LibraryMark {
        self.library.mark(kind, id)
    }

    #[must_use]
    pub fn tile_marks(&self, kind: MediaKind, id: TmdbId) -> TileMarks {
        TileMarks {
            watched: self.is_watched(kind, id),
            library: self.library_mark(kind, id),
        }
    }

    pub fn set_list_status(&mut self, card: &WatchCard, status: Option<ListStatus>) {
        let item = card.item();
        self.library.set_status(&item, card.section, status);
        if let Some(db) = &self.db {
            warn_library_write(db_block_on(db.set_library_status(&item, card.section, status)));
        }
    }

    pub fn set_liked(&mut self, card: &WatchCard, liked: bool) {
        let item = card.item();
        self.library.set_liked(&item, card.section, liked);
        if let Some(db) = &self.db {
            warn_library_write(db_block_on(db.set_library_liked(&item, card.section, liked)));
        }
    }

    pub fn mark_watching_if_unset(&mut self, card: &WatchCard) {
        let item = card.item();
        if !self.library.mark_watching_if_unset(&item, card.section) {
            return;
        }

        if let Some(db) = &self.db {
            warn_library_write(db_block_on(db.mark_watching_if_unset(&item, card.section)));
        }
    }

    pub fn persist(&mut self) {
        self.save_error = None;
        let Some(store) = &self.store else {
            return;
        };

        if let Err(error) = store.save(&self.settings) {
            error!(%error, "failed to save settings");
            self.save_error = Some(error.to_string());
        }
    }

    pub fn clear_tmdb_cache(&mut self) {
        if let Some(db) = &self.db {
            if let Err(error) = db_block_on(db.clear_tmdb()) {
                error!(%error, "failed to clear tmdb cache");
            }
        }
        self.images.clear();
        self.home_needs_refresh = true;
    }

    pub fn take_home_refresh(&mut self) -> bool {
        std::mem::take(&mut self.home_needs_refresh)
    }
}

fn open_app_db(settings: &Settings) -> Option<Arc<Store>> {
    match db_block_on(Store::system()) {
        Ok(store) => {
            let sizes = allowed_image_sizes(settings.tmdb.poster_size);
            if let Err(error) = db_block_on(store.maintenance(&sizes)) {
                warn!(%error, "tmdb cache maintenance failed");
            }
            Some(Arc::new(store))
        }
        Err(error) => {
            error!(%error, "app database unavailable");
            None
        }
    }
}

fn warn_library_write(result: Result<(), StoreError>) {
    if let Err(error) = result {
        warn!(%error, "failed to save library entry");
    }
}

/// Run a database future on the shared Tokio runtime.
///
/// The UI thread is outside that runtime, so `Handle::current()` is not available.
pub(crate) fn db_block_on<T>(fut: impl Future<Output = T>) -> T {
    egui_async::bind::ASYNC_RUNTIME.block_on(fut)
}

fn open_settings_store() -> (Option<SettingsStore>, Settings, Option<String>) {
    let store = match SettingsStore::system() {
        Ok(store) => store,
        Err(error) => {
            error!(%error, "settings store unavailable");
            return (None, Settings::default(), Some(error.to_string()));
        }
    };
    match store.load() {
        Ok(settings) => {
            info!(path = %store.path().display(), "settings loaded");
            if !store.path().exists() {
                if let Err(error) = store.save(&settings) {
                    warn!(%error, "could not write default settings");
                }
            }
            (Some(store), settings, None)
        }
        Err(error) => {
            error!(%error, "failed to load settings");
            (Some(store), Settings::default(), Some(error.to_string()))
        }
    }
}
