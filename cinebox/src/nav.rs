use cinebox_core::{CatalogItem, CreditPerson, LibraryList, MediaKind, Section, TmdbId};
use cinebox_tmdb::ShelfId;

use crate::screens::discover::DiscoverFilters;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Section { section: Section },
    Category { id: ShelfId },
    Discover { section: Section },
    Library,
    Search,
    Media { kind: MediaKind, id: TmdbId },
    Person { id: TmdbId },
    Torrents { kind: MediaKind, id: TmdbId },
    Player { kind: MediaKind, id: TmdbId },
}

/// Top-level destination in the side rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailEntry {
    Home,
    Section(Section),
    Library,
}

impl Screen {
    /// Browse screens get the side rail; detail and playback screens do not.
    #[must_use]
    pub const fn shows_rail(self) -> bool {
        matches!(
            self,
            Self::Home
                | Self::Section { .. }
                | Self::Category { .. }
                | Self::Discover { .. }
                | Self::Library
                | Self::Search
        )
    }

    /// Rail entry highlighted while this screen is shown.
    #[must_use]
    pub const fn rail_entry(self) -> Option<RailEntry> {
        match self {
            Self::Home
            | Self::Category {
                id: ShelfId::Home(_),
            } => Some(RailEntry::Home),
            Self::Section { section }
            | Self::Discover { section }
            | Self::Category {
                id: ShelfId::Section(section, _),
            } => Some(RailEntry::Section(section)),
            Self::Library => Some(RailEntry::Library),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NavAction {
    OpenSettings,
    GoBack,
    OpenRail(RailEntry),
    OpenCategory {
        id: ShelfId,
        items: Vec<CatalogItem>,
    },
    OpenDiscover {
        section: Section,
        filters: DiscoverFilters,
    },
    OpenLibrary {
        list: LibraryList,
    },
    OpenSearch {
        query: String,
    },
    OpenMedia {
        item: CatalogItem,
    },
    OpenPerson {
        person: CreditPerson,
    },
    WatchTorrents,
}

#[derive(Debug, Clone)]
pub struct Nav {
    stack: Vec<Screen>,
}

impl Nav {
    pub fn new() -> Self {
        Self {
            stack: vec![Screen::Home],
        }
    }

    pub fn current(&self) -> Screen {
        self.stack.last().copied().unwrap_or(Screen::Home)
    }

    pub fn push(&mut self, screen: Screen) {
        if self.current() != screen {
            self.stack.push(screen);
        }
    }

    pub fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    /// Jump to a top-level screen: the stack becomes `[Home, screen]`, so Back returns Home.
    pub fn switch_top(&mut self, screen: Screen) {
        self.stack.truncate(1);
        self.push(screen);
    }
}

impl Default for Nav {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_is_root_and_back_stops_there() {
        let mut nav = Nav::new();

        assert_eq!(nav.current(), Screen::Home);

        nav.pop();
        assert_eq!(nav.current(), Screen::Home);

        let movie = Screen::Media {
            kind: MediaKind::Movie,
            id: TmdbId::new(1),
        };

        nav.push(movie);
        assert_eq!(nav.current(), movie);

        nav.push(movie);
        assert_eq!(nav.stack.len(), 2);

        nav.pop();
        assert_eq!(nav.current(), Screen::Home);
    }

    #[test]
    fn media_and_person_stack() {
        let mut nav = Nav::new();
        let movie = Screen::Media {
            kind: MediaKind::Movie,
            id: TmdbId::new(1),
        };

        let person = Screen::Person { id: TmdbId::new(7) };
        let other = Screen::Media {
            kind: MediaKind::Tv,
            id: TmdbId::new(2),
        };

        nav.push(movie);
        nav.push(person);
        nav.push(other);
        assert_eq!(nav.current(), other);

        nav.pop();
        assert_eq!(nav.current(), person);

        nav.pop();
        assert_eq!(nav.current(), movie);

        nav.pop();
        assert_eq!(nav.current(), Screen::Home);
    }

    #[test]
    fn torrents_stack_on_media() {
        let mut nav = Nav::new();
        let movie = Screen::Media {
            kind: MediaKind::Movie,
            id: TmdbId::new(1),
        };

        let torrents = Screen::Torrents {
            kind: MediaKind::Movie,
            id: TmdbId::new(1),
        };

        nav.push(movie);
        nav.push(torrents);
        assert_eq!(nav.current(), torrents);

        nav.pop();
        assert_eq!(nav.current(), movie);
    }

    #[test]
    fn player_stacks_on_torrents() {
        let mut nav = Nav::new();
        let torrents = Screen::Torrents {
            kind: MediaKind::Movie,
            id: TmdbId::new(1),
        };

        let player = Screen::Player {
            kind: MediaKind::Movie,
            id: TmdbId::new(1),
        };

        nav.push(torrents);
        nav.push(player);
        assert_eq!(nav.current(), player);

        nav.pop();
        assert_eq!(nav.current(), torrents);
    }

    #[test]
    fn category_stacks_on_home() {
        let mut nav = Nav::new();
        let category = Screen::Category {
            id: ShelfId::Home(cinebox_core::HomeRowId::NowPlaying),
        };

        nav.push(category);
        assert_eq!(nav.current(), category);

        nav.pop();
        assert_eq!(nav.current(), Screen::Home);
    }

    mod switch_top {
        use super::*;

        #[test]
        fn back_from_switched_section_returns_home() {
            let mut nav = Nav::new();
            nav.switch_top(Screen::Section {
                section: Section::Movies,
            });

            nav.push(Screen::Media {
                kind: MediaKind::Movie,
                id: TmdbId::new(1),
            });

            nav.switch_top(Screen::Section {
                section: Section::Tv,
            });

            nav.pop();
            assert_eq!(nav.current(), Screen::Home);
        }

        #[test]
        fn switching_to_home_leaves_only_root() {
            let mut nav = Nav::new();

            nav.push(Screen::Library);
            nav.switch_top(Screen::Home);

            assert_eq!(nav.stack, vec![Screen::Home]);
        }
    }


    #[test]
    fn search_stacks_on_home_and_does_not_duplicate() {
        let mut nav = Nav::new();

        nav.push(Screen::Search);
        assert_eq!(nav.current(), Screen::Search);
        assert_eq!(nav.stack.len(), 2);

        nav.push(Screen::Search);
        assert_eq!(nav.stack.len(), 2);

        nav.pop();
        assert_eq!(nav.current(), Screen::Home);
    }
}
