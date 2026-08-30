//! Shared services owned by the thin app dispatcher. Screens borrow this.

use std::sync::{Arc, Mutex};

use cinebox_core::{Settings, SettingsStore};
use cinebox_player::Engine;
use tracing::{error, info, warn};

use crate::images::ImageCache;
use crate::toasts::Toasts;

pub struct Services {
    pub settings: Settings,
    pub store: Option<SettingsStore>,
    pub load_error: Option<String>,
    pub save_error: Option<String>,
    pub images: ImageCache,
    pub toasts: Toasts,
    pub engine: Option<Arc<Mutex<Engine>>>,
}

impl Services {
    pub fn boot(engine: Option<Arc<Mutex<Engine>>>) -> Self {
        let (store, settings, load_error) = open_settings_store();
        Self {
            settings,
            store,
            load_error,
            save_error: None,
            images: ImageCache::new(),
            toasts: Toasts::default(),
            engine,
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
            if !store.path().exists()
                && let Err(error) = store.save(&settings)
            {
                warn!(%error, "could not write default settings");
            }
            (Some(store), settings, None)
        }
        Err(error) => {
            error!(%error, "failed to load settings");
            (Some(store), Settings::default(), Some(error.to_string()))
        }
    }
}
