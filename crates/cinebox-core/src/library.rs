//! User lists: one watch status per title plus an independent "liked" flag.

use serde::{Deserialize, Serialize};

use crate::catalog::CatalogItem;
use crate::section::Section;

/// Mutually exclusive watch status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListStatus {
    Watching,
    Planned,
    Completed,
    Dropped,
}

impl ListStatus {
    pub const ALL: [Self; 4] = [Self::Watching, Self::Planned, Self::Completed, Self::Dropped];

    /// Stable key for the `library.status` column.
    #[must_use]
    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Watching => "watching",
            Self::Planned => "planned",
            Self::Completed => "completed",
            Self::Dropped => "dropped",
        }
    }

    /// Parse a key written by [`ListStatus::as_key`].
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_key() == key)
    }
}

/// What the user has set on one title.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LibraryMark {
    pub status: Option<ListStatus>,
    pub liked: bool,
}

impl LibraryMark {
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.status.is_none() && !self.liked
    }

    #[must_use]
    pub fn in_list(self, list: LibraryList) -> bool {
        match list {
            LibraryList::Status(status) => self.status == Some(status),
            LibraryList::Liked => self.liked,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibraryList {
    Status(ListStatus),
    Liked,
}

impl LibraryList {
    pub const ALL: [Self; 5] = [
        Self::Status(ListStatus::Watching),
        Self::Status(ListStatus::Planned),
        Self::Status(ListStatus::Completed),
        Self::Status(ListStatus::Dropped),
        Self::Liked,
    ];
}

/// One stored library row, newest first when listed.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntry {
    pub item: CatalogItem,
    pub section: Section,
    pub mark: LibraryMark,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_keys_round_trip() {
        for status in ListStatus::ALL {
            assert_eq!(ListStatus::from_key(status.as_key()), Some(status));
        }
    }
}
