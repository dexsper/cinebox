//! TorrServer HTTP client: echo, speed test, torrents, stream URLs, viewed, preload.

#![forbid(unsafe_code)]

mod auth;
mod cache;
mod card;
mod client;
mod discover;
mod episode;
mod error;
mod probe;
mod server;
mod status;
mod stream;
mod torrents;
mod viewed;

pub use auth::basic_auth_header;
pub use cache::{CacheState, PieceState, ReaderState, ResumeProgress, cache_state, resume_window_progress};
pub use card::{TorrCard, decode_data, encode_data, stored_files, with_card};
pub use discover::{FoundServer, discover};
pub use episode::{FileEpisode, file_display_name, parse_file_episode};
pub use error::Error;
pub use probe::{SPEED_TEST_FILE_MB, SpeedEvent, SpeedReport, echo, speed_test};
pub use server::Server;
pub use status::{FileStat, TorrentStat, TorrentStatus, files_for_list, is_playable_path};
pub use stream::{PreloadEvent, PreloadTarget, StreamFlag, play_url, stream_url, wait_preload, wait_preload_at_bytes};
pub use torrents::{
    AddSpec, ListedTorrent, MetaSpec, OpenedFile, OpenedTorrent, add, drop_torrent, get, list, open_magnet, set_meta,
    wait_files,
};
pub use viewed::{Viewed, is_foreign_timecode, merge_timecode, viewed_all, viewed_list, viewed_set};
