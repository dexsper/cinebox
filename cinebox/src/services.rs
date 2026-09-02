//! Shared services owned by the thin app dispatcher. Screens borrow this.

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, Mutex};

use cinebox_core::{MediaKind, Settings, SettingsStore, Store, TmdbId, allowed_image_sizes};
use cinebox_player::Engine;
use tracing::{error, info, warn};

use crate::images::ImageCache;
use crate::toasts::Toasts;

pub struct Services {
    pub settings: Settings,
    pub store: Option<SettingsStore>,
    pub db: Option<Arc<Store>>,
    pub load_error: Option<String>,
    pub save_error: Option<String>,
    pub images: ImageCache,
    pub toasts: Toasts,
    pub engine: Option<Arc<Mutex<Engine>>>,
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

        Self {
            settings,
            store,
            db,
            load_error,
            save_error: None,
            images,
            toasts: Toasts::default(),
            engine,
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
