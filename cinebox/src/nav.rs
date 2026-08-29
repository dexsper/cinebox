use cinebox_core::{MediaKind, TmdbId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Settings,
    Media { kind: MediaKind, id: TmdbId },
    Person { id: TmdbId },
    Torrents { kind: MediaKind, id: TmdbId },
    Player { kind: MediaKind, id: TmdbId },
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
        nav.push(Screen::Settings);
        assert_eq!(nav.current(), Screen::Settings);
        nav.push(Screen::Settings);
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
}
