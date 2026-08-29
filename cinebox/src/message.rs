//! Global iced message hub. UI modules import [`crate::app::Message`].

use cinebox_core::{HomeCatalog, MediaDetails, MediaKind, PersonDetails, TmdbId};
use cinebox_parse::TorrentHit;

use crate::ui;
use crate::ui::scroll::ScrollPane;

#[derive(Debug, Clone)]
pub enum Message {
    OpenSettings,
    GoBack,
    RetryHome,
    RetryMedia,
    RetryPerson,
    OpenMedia {
        kind: MediaKind,
        id: TmdbId,
    },
    OpenPerson {
        id: TmdbId,
    },
    WatchTorrents,
    RetryTorrents,
    Torrents(ui::torrents::Event),
    TorrentsLoaded {
        kind: MediaKind,
        id: TmdbId,
        result: Result<Vec<TorrentHit>, String>,
    },
    OpenUrl(String),
    Settings(ui::settings::Message),
    HomeLoaded(Result<HomeCatalog, String>),
    MediaLoaded {
        kind: MediaKind,
        id: TmdbId,
        result: Result<Box<MediaDetails>, String>,
    },
    PersonLoaded {
        id: TmdbId,
        result: Result<Box<PersonDetails>, String>,
    },
    PosterLoaded {
        key: (MediaKind, TmdbId),
        result: Result<Vec<u8>, String>,
    },
    ImageLoaded {
        url: String,
        result: Result<Vec<u8>, String>,
    },
    ScrollPan {
        pane: ScrollPane,
        dx: f32,
    },
    ScrollImpulse {
        pane: ScrollPane,
        dx: f32,
        dy: f32,
        gain: f32,
    },
    ScrollFlick {
        pane: ScrollPane,
        vx: f32,
        vy: f32,
    },
    ScrollDragging(bool),
    ScrollFrame(std::time::Instant),
}
