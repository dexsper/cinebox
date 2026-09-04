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

/// Stream origin for [`PlayRequest`].
#[derive(Debug, Clone)]
pub enum PlaySource {
    Torrent {
        hash: String,
        files: Vec<TorrentFileRow>,
        file_index: usize,
        start: f64,
    },
    Youtube {
        video_url: String,
        audio_url: Option<String>,
        http_header_fields: Vec<String>,
    },
}

impl PlaySource {
    #[must_use]
    pub fn is_youtube(&self) -> bool {
        matches!(self, Self::Youtube { .. })
    }

    #[must_use]
    pub fn torrent_hash(&self) -> Option<&str> {
        match self {
            Self::Torrent { hash, .. } => Some(hash.as_str()),
            Self::Youtube { .. } => None,
        }
    }

    #[must_use]
    pub fn files(&self) -> &[TorrentFileRow] {
        match self {
            Self::Torrent { files, .. } => files,
            Self::Youtube { .. } => &[],
        }
    }

    #[must_use]
    pub fn file_index(&self) -> usize {
        match self {
            Self::Torrent { file_index, .. } => *file_index,
            Self::Youtube { .. } => 0,
        }
    }

    #[must_use]
    pub fn has_next(&self) -> bool {
        match self {
            Self::Torrent { files, file_index, .. } => file_index + 1 < files.len(),
            Self::Youtube { .. } => false,
        }
    }

    #[must_use]
    pub fn start_seconds(&self) -> f64 {
        match self {
            Self::Torrent { start, .. } => *start,
            Self::Youtube { .. } => 0.0,
        }
    }

    pub fn set_current_timecode(&mut self, time: f64) {
        let Self::Torrent {
            files,
            file_index,
            ..
        } = self
        else {
            return;
        };

        let Some(file) = files.get_mut(*file_index) else {
            return;
        };

        file.timecode = time;
    }
}

pub struct PlayRequest {
    pub card: WatchCard,
    pub title: String,
    pub backdrop_path: Option<String>,
    pub source: PlaySource,
}
