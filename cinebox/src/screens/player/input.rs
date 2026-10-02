//! Keyboard shortcuts, escape handling, and OS fullscreen bookkeeping.

use cinebox_player::SEEK_SECS;
use egui::{Id, Key, Response, Ui, Vec2, ViewportCommand};

use crate::platform;
use crate::services::Services;
use crate::widgets::focus;

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

        // Back from the controls returns the D-pad to the video before leaving.
        if self.controls_focused && platform::profile(ctx).is_directional() {
            self.controls_focused = false;
            self.activity.hide();
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

        // With a D-pad the arrows also walk the controls; they seek only from the video.
        let directional = platform::profile(ui.ctx()).is_directional();
        let on_video = ui.ctx().memory(|mem| mem.has_focus(video_id()));
        if directional && !on_video {
            return;
        }

        if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.seek(svc, -SEEK_SECS);
        }

        if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.seek(svc, SEEK_SECS);
        }
    }

    /// The video holds D-pad focus while nothing else does, so the arrows seek
    /// and OK pauses. `true` when Up/Down asked for the controls.
    pub(super) fn dpad_video_focus(
        &mut self,
        ui: &Ui,
        video: &Response,
        popup_open: bool,
        now: f64,
    ) -> bool {
        if !platform::profile(ui.ctx()).is_directional() {
            return false;
        }

        focus::hold_arrows(ui, video.id, true, true);

        let unfocused = ui.ctx().memory(|mem| mem.focused()).is_none();
        if unfocused && !popup_open {
            video.request_focus();
            return false;
        }

        if !video.has_focus() {
            return false;
        }

        let vertical = ui.input(|i| i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::ArrowDown));
        if vertical {
            self.activity.poke(now);
        }

        vertical
    }

    pub(super) fn update_activity(&mut self, ui: &Ui, now: f64) {
        let interacted = ui.input(|i| {
            i.pointer.delta() != Vec2::ZERO
                || i.pointer.any_down()
                || i.smooth_scroll_delta != Vec2::ZERO
                || i.events.iter().any(is_activity_key)
        });

        if interacted {
            self.activity.poke(now);
        }
    }

    pub(super) fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        if self.fullscreen == on {
            return;
        }

        // A window the OS owns is always full screen.
        if !platform::profile(ctx).is_desktop_window() {
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

/// Any key shows the controls, except Back, which may be hiding them.
fn is_activity_key(event: &egui::Event) -> bool {
    let egui::Event::Key { key, .. } = event else {
        return false;
    };

    *key != Key::Escape
}

/// The video surface; on TV it is the D-pad's resting place.
pub(super) fn video_id() -> Id {
    Id::new("player-video")
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
        assert!(screen.fullscreen);

        assert!(screen.consume_escape(&ctx));
        assert!(!screen.fullscreen);

        assert!(!screen.consume_escape(&ctx));
    }
}
