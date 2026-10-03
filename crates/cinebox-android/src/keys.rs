//! Remote keys egui has no name for.
//!
//! winit reports media and search keys as handled, so Android never routes
//! them to the media session, and egui-winit drops them. This wrapper around
//! eframe's winit application turns them into device events first.

use cinebox::platform::{DeviceEvent, MediaCommand};
use cinebox_player::SEEK_SECS;
use eframe::UserEvent;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent as WinitDeviceEvent, DeviceId, ElementState, KeyEvent, StartCause, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

use crate::events;

pub struct KeyRouter<A> {
    app: A,
}

impl<A> KeyRouter<A> {
    pub fn new(app: A) -> Self {
        Self { app }
    }
}

fn device_event(key: &KeyEvent) -> Option<DeviceEvent> {
    if key.state != ElementState::Pressed {
        return None;
    }

    if key.repeat {
        return None;
    }

    let Key::Named(named) = key.logical_key else {
        return None;
    };

    let command = match named {
        NamedKey::BrowserSearch => return Some(DeviceEvent::SearchKey),
        NamedKey::MediaPlayPause => MediaCommand::PlayPause,
        NamedKey::MediaPlay => MediaCommand::Play,
        NamedKey::MediaPause => MediaCommand::Pause,
        NamedKey::MediaStop => MediaCommand::Stop,
        NamedKey::MediaFastForward => MediaCommand::SeekBy(SEEK_SECS),
        NamedKey::MediaRewind => MediaCommand::SeekBy(-SEEK_SECS),
        NamedKey::MediaTrackNext => MediaCommand::Next,
        NamedKey::MediaTrackPrevious => MediaCommand::Previous,
        _ => return None,
    };

    Some(DeviceEvent::Media(command))
}

impl<A: ApplicationHandler<UserEvent>> ApplicationHandler<UserEvent> for KeyRouter<A> {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.app.new_events(event_loop, cause);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.app.resumed(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        self.app.user_event(event_loop, event);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::KeyboardInput { event: key, .. } = &event {
            if let Some(device_event) = device_event(key) {
                events::push(device_event);
            }
        }

        self.app.window_event(event_loop, id, event);
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: WinitDeviceEvent) {
        self.app.device_event(event_loop, id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.app.about_to_wait(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.app.suspended(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.app.exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.app.memory_warning(event_loop);
    }
}
