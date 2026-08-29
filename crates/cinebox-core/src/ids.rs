//! Domain identifiers.

use serde::{Deserialize, Serialize};

/// TMDB resource id. Distinct from torrent hashes and file indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TmdbId(u32);

impl TmdbId {
    /// Wrap a TMDB numeric id.
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Inner TMDB id.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Kind of catalog item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Movie,
    Tv,
    Person,
}
