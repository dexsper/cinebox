//! Top-level catalog sections.

use serde::{Deserialize, Serialize};

use crate::ids::MediaKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Movies,
    Cartoons,
    Tv,
    Anime,
}

impl Section {
    pub const ALL: [Self; 4] = [Self::Movies, Self::Cartoons, Self::Tv, Self::Anime];

    /// Stable key for SQL columns and cache ids.
    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Movies => "movies",
            Self::Cartoons => "cartoons",
            Self::Tv => "tv",
            Self::Anime => "anime",
        }
    }

    /// Parse a key written by [`Section::as_key`].
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.as_key() == key)
    }

    /// Section when only the media kind is known (history rows saved before sections existed).
    #[must_use]
    pub const fn fallback(kind: MediaKind) -> Self {
        match kind {
            MediaKind::Tv => Self::Tv,
            MediaKind::Movie | MediaKind::Person => Self::Movies,
        }
    }

    /// Kind most of this section's shelves list.
    #[must_use]
    pub const fn primary_kind(self) -> MediaKind {
        match self {
            Self::Movies | Self::Cartoons => MediaKind::Movie,
            Self::Tv | Self::Anime => MediaKind::Tv,
        }
    }

    /// Kinds the discover screen can switch between, primary first.
    #[must_use]
    pub const fn kinds(self) -> &'static [MediaKind] {
        match self {
            Self::Movies => &[MediaKind::Movie],
            Self::Tv => &[MediaKind::Tv],
            Self::Cartoons => &[MediaKind::Movie, MediaKind::Tv],
            Self::Anime => &[MediaKind::Tv, MediaKind::Movie],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for section in Section::ALL {
            assert_eq!(Section::from_key(section.as_key()), Some(section));
        }
    }
}
