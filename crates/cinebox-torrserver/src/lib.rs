//! TorrServer HTTP client: echo, speed test, torrents, stream URLs, viewed, preload.

#![forbid(unsafe_code)]

mod auth;
mod cache;
mod client;
mod error;
mod probe;
mod status;
mod stream;
mod torrents;
mod viewed;

pub use auth::mpv_http_header_fields;
pub use cache::{CacheState, PieceState, ReaderState, ResumeProgress, cache_state, resume_window_progress};
pub use error::Error;
pub use probe::{SPEED_TEST_FILE_MB, SpeedEvent, SpeedReport, echo, speed_test};
pub use status::{FileStat, TorrentStat, TorrentStatus, files_for_list, is_playable_path};
pub use stream::{
    PreloadEvent, PreloadTarget, StreamFlag, play_url, stream_url, wait_preload,
    wait_preload_at_bytes,
};
pub use torrents::{
    AddSpec, ListedTorrent, OpenedFile, OpenedTorrent, add, drop_torrent, get, list, open_magnet,
    wait_files,
};
pub use viewed::{Viewed, viewed_list, viewed_set};
