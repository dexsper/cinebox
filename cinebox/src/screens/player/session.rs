//! The player as the OS media session sees it.

use std::sync::Arc;

use cinebox_core::{MediaKind, TmdbId, normalize_tmdb_path};

use super::PlayerState;
use crate::platform::{self, MediaSessionState, SessionPublisher};
use crate::screens::play::WatchCard;
use crate::services::{Services, db_block_on};

#[derive(Default)]
pub(super) struct MediaSession {
    publisher: SessionPublisher,
    artwork: Option<Artwork>,
}

struct Artwork {
    card: (MediaKind, TmdbId),
    image: Option<Arc<[u8]>>,
}

impl MediaSession {
    pub(super) fn publish(&mut self, svc: &Services, state: &PlayerState, ctx: &egui::Context) {
        let artwork = self.artwork_for(svc, &state.card);
        let subtitle = (state.card.title != state.title).then(|| state.card.title.clone());
        let session = MediaSessionState {
            title: state.title.clone(),
            subtitle,
            duration: state.duration,
            artwork,
            playing: !state.paused,
            position: state.time,
            can_next: state.has_next(),
            can_previous: state.file_index() > 0,
        };

        let now = ctx.input(|i| i.time);
        let device = platform::device(ctx);
        self.publisher.publish(device.as_ref(), session, now);
    }

    pub(super) fn clear(&mut self, ctx: &egui::Context) {
        let device = platform::device(ctx);
        self.publisher.clear(device.as_ref());
    }

    fn artwork_for(&mut self, svc: &Services, card: &WatchCard) -> Option<Arc<[u8]>> {
        let key = (card.kind, card.id);
        if let Some(artwork) = &self.artwork {
            if artwork.card == key {
                return artwork.image.clone();
            }
        }

        let image = cached_poster(svc, card);
        self.artwork = Some(Artwork {
            card: key,
            image: image.clone(),
        });
        image
    }
}

/// Only what the image store already holds (the media page showed the poster):
/// TMDB may be reachable only through the app's own proxy settings, so the OS
/// is never handed a URL to fetch.
fn cached_poster(svc: &Services, card: &WatchCard) -> Option<Arc<[u8]>> {
    let path = normalize_tmdb_path(card.poster_path.as_deref())?;
    let db = svc.db.as_ref()?;
    let size = svc.settings.tmdb.poster_size.tmdb_path();
    let bytes = db_block_on(db.get_image(size, &path)).ok().flatten()?;

    Some(Arc::from(bytes))
}
