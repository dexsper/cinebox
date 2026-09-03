//! Keyboard shortcuts, escape handling, and OS fullscreen bookkeeping.

use cinebox_player::SEEK_SECS;
use egui::{Ui, Vec2, ViewportCommand};

use crate::services::Services;

use super::{PlayerPhase, PlayerScreen, Popup};

impl PlayerScreen {
    /// Escape while the player is on screen: close a popup first, then exit
    /// fullscreen. `true` when consumed (navigation must not pop).
    pub fn consume_escape(&mut self, ctx: &egui::Context) -> bool {
        if self.popup != Popup::None {
            self.popup = Popup::None;
            return true;
        }

        if self.fullscreen {
            self.set_fullscreen(ctx, false);
            return true;
        }

        false
    }

    pub(super) fn handle_keys(&mut self, ui: &Ui, svc: &Services) {
        if !matches!(self.phase, Some(PlayerPhase::Playing(_))) {
            return;
        }

        if ui.input(|i| i.key_pressed(egui::Key::Space)) {
            self.toggle(svc);
        }

        if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.seek(svc, -SEEK_SECS);
        }

        if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.seek(svc, SEEK_SECS);
        }
    }

    pub(super) fn update_activity(&mut self, ui: &Ui, now: f64) {
        let interacted = ui.input(|i| {
            i.pointer.delta() != Vec2::ZERO
                || i.pointer.any_down()
                || i.smooth_scroll_delta != Vec2::ZERO
                || i.events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Key { .. }))
        });

        if interacted {
            self.activity.poke(now);
        }
    }

    pub(super) fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        if self.fullscreen == on {
            return;
        }

        self.fullscreen = on;
        if on {
            // A maximized undecorated window overhangs the screen edges on
            // Windows; going borderless-fullscreen from it keeps that stale
            // geometry (footer lands below the screen). Un-maximize first.
            self.was_maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
            if self.was_maximized {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
            }
            
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
            return;
        }

        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        if self.was_maximized {
            self.was_maximized = false;
            ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
        }
    }

    pub(super) fn sync_fullscreen(&mut self, ctx: &egui::Context) {
        if !self.fullscreen {
            return;
        }

        let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
        if maximized {
            self.was_maximized = true;
            ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
        }

        let fullscreen = ctx.input(|i| i.viewport().fullscreen).unwrap_or(true);
        if !fullscreen {
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{PlayerScreen, Popup};

    #[test]
    fn escape_closes_popup_before_leaving_fullscreen() {
        let ctx = egui::Context::default();
        let mut screen = PlayerScreen {
            fullscreen: true,
            popup: Popup::Playlist,
            ..PlayerScreen::default()
        };

        assert!(screen.consume_escape(&ctx));
        assert!(screen.popup == Popup::None);
        assert!(screen.is_fullscreen());

        assert!(screen.consume_escape(&ctx));
        assert!(!screen.is_fullscreen());

        assert!(!screen.consume_escape(&ctx));
    }
}
