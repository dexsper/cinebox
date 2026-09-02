use cinebox_core::{MediaKind, TmdbId};

use crate::screens::torrents::TorrentFileRow;

/// Denormalized TMDB card so watch history never depends on the cache.
#[derive(Debug, Clone)]
pub struct WatchCard {
    pub kind: MediaKind,
    pub id: TmdbId,
    pub title: String,
    pub poster_path: Option<String>,
    pub year: Option<u16>,
    pub vote: Option<f32>,
}

pub struct PlayRequest {
    pub card: WatchCard,
    pub title: String,
    pub hash: String,
    pub files: Vec<TorrentFileRow>,
    pub file_index: usize,
    pub start: f64,
    pub backdrop_path: Option<String>,
}
