//! What the screen and the input are like, as far as the layout cares.

use egui::Margin;

/// How the user reaches a widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    /// Mouse or touchpad: hover, click, wheel.
    Pointer,
    /// A remote's D-pad moves a visible focus; OK activates.
    Directional,
}

/// Who owns the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Windowing {
    /// The app draws its own title bar and can be moved, resized and made fullscreen.
    Desktop,
    /// The OS shows the app full screen. Back from the first screen leaves the
    /// app, and playback pauses while the app is hidden.
    System,
}

/// How far the viewer sits from the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewing {
    /// At a desk, or the device in hand.
    Near,
    /// Across the room: the header and its search are set larger.
    Far,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    pub navigation: Navigation,
    pub windowing: Windowing,
    pub viewing: Viewing,
    /// Kept clear of controls along the screen edges; backgrounds still reach them.
    pub edge_inset: Margin,
    /// Logical width the layout is scaled to, where the OS density alone gets it wrong.
    pub layout_width: Option<f32>,
}

impl Profile {
    #[must_use]
    pub const fn desktop() -> Self {
        Self {
            navigation: Navigation::Pointer,
            windowing: Windowing::Desktop,
            viewing: Viewing::Near,
            edge_inset: Margin::ZERO,
            layout_width: None,
        }
    }

    /// 1080p TVs report density 2.0, which alone leaves 960x540 points, and
    /// many crop a few percent of the picture at the edges.
    #[must_use]
    pub const fn tv() -> Self {
        Self {
            navigation: Navigation::Directional,
            windowing: Windowing::System,
            viewing: Viewing::Far,
            edge_inset: Margin::symmetric(24, 16),
            layout_width: Some(1280.0),
        }
    }

    #[must_use]
    pub fn is_directional(self) -> bool {
        self.navigation == Navigation::Directional
    }

    #[must_use]
    pub fn is_far(self) -> bool {
        self.viewing == Viewing::Far
    }

    #[must_use]
    pub fn is_desktop_window(self) -> bool {
        self.windowing == Windowing::Desktop
    }

    /// The OS has a Back of its own (a remote's key, a system bar), so the
    /// app draws none.
    #[must_use]
    pub fn has_system_back(self) -> bool {
        self.windowing == Windowing::System
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self::desktop()
    }
}
