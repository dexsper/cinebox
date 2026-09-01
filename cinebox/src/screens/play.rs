use cinebox_core::{MediaKind, TmdbId};

use crate::screens::torrents::TorrentFileRow;

pub struct PlayRequest {
    pub kind: MediaKind,
    pub id: TmdbId,
    pub title: String,
    pub hash: String,
    pub files: Vec<TorrentFileRow>,
    pub file_index: usize,
    pub start: f64,
    pub backdrop_path: Option<String>,
}
