//! Keeps the screens' TMDB data in step with the settings it was fetched with.

use cinebox_core::{PosterSize, Settings, UiLanguage, allowed_image_sizes};
use tracing::error;

use super::App;
use crate::screens::LiveTmdb;
use crate::services::db_block_on;
use crate::services::jobs;
use crate::services::settings_input::tmdb_key_hint;

/// The settings TMDB data depends on, as of the last sync.
pub(super) struct TmdbView {
    api_key: String,
    language: UiLanguage,
    poster_size: PosterSize,
    net: cinebox_net::NetConfig,
}

#[derive(Debug, PartialEq, Eq)]
enum TmdbChange {
    None,
    /// The key was removed: nothing can be fetched, cached TMDB data goes too.
    KeyCleared,
    /// Key or network changed: refetch everything, images included.
    Catalog,
    Language,
    PosterSize,
}

impl TmdbView {
    pub(super) fn from_settings(settings: &Settings) -> Self {
        Self {
            api_key: settings.tmdb.api_key.expose().to_owned(),
            language: settings.general.language,
            poster_size: settings.tmdb.poster_size,
            net: jobs::net_config(settings),
        }
    }

    /// Checked every frame, so without cloning the api key.
    fn matches(&self, settings: &Settings) -> bool {
        self.api_key == settings.tmdb.api_key.expose()
            && self.language == settings.general.language
            && self.poster_size == settings.tmdb.poster_size
            && self.net == jobs::net_config(settings)
    }

    fn change_from(&self, next: &Self) -> TmdbChange {
        let key_cleared = next.api_key.is_empty() && !self.api_key.is_empty();
        if key_cleared {
            return TmdbChange::KeyCleared;
        }

        // Without a usable key there is nothing to reload; a malformed one would
        // only earn a 401 and a flash of the error page.
        let key_unusable = next.api_key.is_empty() || tmdb_key_hint(&next.api_key).is_some();
        if key_unusable {
            return TmdbChange::None;
        }

        let key_changed = next.api_key != self.api_key;
        let language_changed = next.language != self.language;
        let net_changed = next.net != self.net;

        if key_changed || net_changed {
            return TmdbChange::Catalog;
        }

        if language_changed {
            return TmdbChange::Language;
        }

        if next.poster_size != self.poster_size {
            return TmdbChange::PosterSize;
        }

        TmdbChange::None
    }
}

impl App {
    pub(super) fn sync_tmdb(&mut self) {
        if self.last_tmdb.matches(&self.services.settings) {
            return;
        }

        let next = TmdbView::from_settings(&self.services.settings);
        if next.language != self.last_tmdb.language {
            crate::i18n::apply(next.language);
        }

        let change = self.last_tmdb.change_from(&next);
        self.last_tmdb = next;

        match change {
            TmdbChange::KeyCleared => {
                self.forget_live_tmdb();
                self.services.clear_tmdb_cache();
            }
            TmdbChange::Catalog => {
                self.forget_live_tmdb();
                self.services.images.clear();
            }
            TmdbChange::Language => self.forget_live_tmdb(),
            TmdbChange::PosterSize => self.gc_poster_images(),
            TmdbChange::None => {}
        }
    }

    fn live_tmdb_screens(&mut self) -> [&mut dyn LiveTmdb; 7] {
        [
            &mut self.home,
            &mut self.section,
            &mut self.category,
            &mut self.discover,
            &mut self.search,
            &mut self.media,
            &mut self.person,
        ]
    }

    fn forget_live_tmdb(&mut self) {
        for screen in self.live_tmdb_screens() {
            screen.forget_live();
        }
    }

    fn gc_poster_images(&mut self) {
        self.services.images.clear();
        let Some(db) = &self.services.db else {
            return;
        };

        let sizes = allowed_image_sizes(self.services.settings.tmdb.poster_size);
        if let Err(error) = db_block_on(db.gc_images(&sizes)) {
            error!(%error, "failed to gc tmdb images");
        }
    }
}

#[cfg(test)]
mod change_from {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    fn view(api_key: &str) -> TmdbView {
        TmdbView {
            api_key: api_key.to_owned(),
            language: UiLanguage::English,
            poster_size: PosterSize::W500,
            net: cinebox_net::NetConfig::default(),
        }
    }

    #[test]
    fn removing_the_key_clears_the_cache() {
        let change = view(KEY).change_from(&view(""));

        assert_eq!(change, TmdbChange::KeyCleared);
    }

    #[test]
    fn a_malformed_key_reloads_nothing() {
        let change = view(KEY).change_from(&view("not-a-key"));

        assert_eq!(change, TmdbChange::None);
    }

    #[test]
    fn a_new_key_refetches_the_catalog() {
        let change = view(KEY).change_from(&view("fedcba9876543210fedcba9876543210"));

        assert_eq!(change, TmdbChange::Catalog);
    }

    #[test]
    fn a_new_network_setting_refetches_the_catalog() {
        let next = TmdbView {
            net: cinebox_net::NetConfig {
                dns_bypass: true,
                ..cinebox_net::NetConfig::default()
            },
            ..view(KEY)
        };

        assert_eq!(view(KEY).change_from(&next), TmdbChange::Catalog);
    }

    #[test]
    fn a_new_language_refetches_texts_only() {
        let next = TmdbView {
            language: UiLanguage::Russian,
            ..view(KEY)
        };

        assert_eq!(view(KEY).change_from(&next), TmdbChange::Language);
    }

    #[test]
    fn a_new_poster_size_keeps_the_catalog() {
        let next = TmdbView {
            poster_size: PosterSize::W342,
            ..view(KEY)
        };

        assert_eq!(view(KEY).change_from(&next), TmdbChange::PosterSize);
    }
}
