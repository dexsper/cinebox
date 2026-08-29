//! Navigation stack. Home is always the root.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Settings,
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
}
