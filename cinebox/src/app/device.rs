//! Input from the device rather than the window: remote, voice, app focus.

use egui::Context;

use super::App;
use super::nav::Screen;
use crate::platform::{DeviceEvent, MediaCommand, SpeechEvent};

impl App {
    /// Home button or another app on top: the film should not keep playing unseen.
    pub(super) fn pause_in_background(&mut self, ctx: &Context) {
        let focused = ctx.input(|i| i.viewport().focused);
        let backgrounded = focused == Some(false) && self.foreground;
        self.foreground = focused != Some(false);

        let on_player = matches!(self.nav.current(), Screen::Player { .. });
        if backgrounded && on_player {
            self.player.pause(&self.services);
        }
    }

    pub(super) fn handle_device_events(&mut self, ctx: &Context) {
        for event in std::mem::take(&mut self.device_events) {
            match event {
                DeviceEvent::Media(command) => self.media_command(command, ctx),
                DeviceEvent::Speech(speech) => self.voice_search(speech, ctx),
                DeviceEvent::SearchKey => self.search_bar.start_search(ctx),
                // Already turned into input for the focused field by `take_input`.
                DeviceEvent::Text(_) => {}
            }
        }
    }

    fn media_command(&mut self, command: MediaCommand, ctx: &Context) {
        let on_player = matches!(self.nav.current(), Screen::Player { .. });
        if on_player {
            self.player.apply(command, &mut self.services, ctx);
        }
    }

    fn voice_search(&mut self, speech: SpeechEvent, ctx: &Context) {
        let Some(action) = self.search_bar.on_speech(speech) else {
            return;
        };

        self.apply_nav(action, ctx.input(|i| i.time), ctx);
    }
}
