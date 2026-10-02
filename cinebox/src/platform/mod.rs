//! The seam between the shared app and the platform it runs on: what the
//! screen and input are like ([`Profile`]) and what the OS does for the app
//! ([`Device`]). Widgets ask for a capability, never for a platform, so the
//! desktop tests can drive the TV layout too.

mod device;
mod media_session;
mod profile;
pub(crate) mod text_input;

use std::sync::Arc;

use cinebox_core::Overscan;
use egui::{Context, Id, Key, Margin, RawInput};

pub use device::{
    Device, DeviceEvent, Direction, MediaCommand, MediaSessionState, NoDevice, SpeechEvent,
    SpeechRequest, TextAction, TextInputEvent, TextInputSpec, TextPurpose, UiSound,
};
pub(crate) use media_session::SessionPublisher;
pub use profile::{Navigation, Profile, Viewing, Windowing};

/// What a platform entry point hands the shared app.
#[derive(Clone)]
pub struct Host {
    pub profile: Profile,
    pub device: Arc<dyn Device>,
}

impl Host {
    #[must_use]
    pub fn desktop() -> Self {
        Self {
            profile: Profile::desktop(),
            device: Arc::new(NoDevice),
        }
    }
}

fn profile_id() -> Id {
    Id::new("cinebox-platform-profile")
}

fn device_id() -> Id {
    Id::new("cinebox-platform-device")
}

fn overscan_id() -> Id {
    Id::new("cinebox-platform-overscan")
}

pub(crate) fn install(ctx: &Context, host: Host) {
    host.device.attach(ctx);
    ctx.data_mut(|data| {
        data.insert_temp(profile_id(), host.profile);
        data.insert_temp(device_id(), host.device);
    });
}

#[must_use]
pub(crate) fn profile(ctx: &Context) -> Profile {
    let profile = ctx.data(|data| data.get_temp::<Profile>(profile_id()));
    profile.unwrap_or_default()
}

#[must_use]
pub(crate) fn device(ctx: &Context) -> Arc<dyn Device> {
    let device = ctx.data(|data| data.get_temp::<Arc<dyn Device>>(device_id()));
    device.unwrap_or_else(|| Arc::new(NoDevice))
}

/// The user's calibration of how much the screen crops off.
pub(crate) fn set_overscan(ctx: &Context, overscan: Overscan) {
    ctx.data_mut(|data| data.insert_temp(overscan_id(), overscan));
}

/// Kept clear of controls along the screen edges; backgrounds still reach them.
#[must_use]
pub(crate) fn edge_inset(ctx: &Context) -> Margin {
    if !profile(ctx).overscan {
        return Margin::ZERO;
    }

    let overscan = ctx.data(|data| data.get_temp::<Overscan>(overscan_id()));
    let share = overscan.unwrap_or_default().fraction();
    let cropped = ctx.content_rect().size() * share;

    Margin::symmetric(cropped.x.round() as i8, cropped.y.round() as i8)
}

/// Before egui sees the frame's input: the OS Back becomes Escape, which
/// popups, fields and navigation already handle, and on-screen keyboard typing
/// becomes egui input. Returns the device events the app itself acts on.
pub(crate) fn take_input(ctx: &Context, raw_input: &mut RawInput) -> Vec<DeviceEvent> {
    if profile(ctx).windowing == Windowing::System {
        back_is_escape(raw_input);
    }

    text_input::begin_input(ctx, raw_input);
    let device = device(ctx);
    let mut for_app = Vec::new();
    while let Some(event) = device.poll_event() {
        match event {
            DeviceEvent::Text(text) => text_input::feed(ctx, raw_input, text),
            other => for_app.push(other),
        }
    }

    for_app
}

fn back_is_escape(raw_input: &mut RawInput) {
    for event in &mut raw_input.events {
        let egui::Event::Key { key, .. } = event else {
            continue;
        };

        if *key == Key::BrowserBack {
            *key = Key::Escape;
        }
    }
}

/// Per-frame upkeep before the app's own logic.
pub(crate) fn begin_frame(ctx: &Context) {
    if let Some(width) = profile(ctx).layout_width {
        fit_layout_width(ctx, width);
    }
}

/// After every widget of the frame is shown.
pub(crate) fn end_frame(ctx: &Context) {
    text_input::sync(ctx);
}

fn fit_layout_width(ctx: &Context, width_pt: f32) {
    let Some(native) = ctx.input(|i| i.viewport().native_pixels_per_point) else {
        return;
    };

    if native <= 0.0 {
        return;
    }

    let width_px = ctx.content_rect().width() * ctx.pixels_per_point();
    if width_px <= 0.0 {
        return;
    }

    let zoom = width_px / width_pt / native;
    if (ctx.zoom_factor() - zoom).abs() > 0.01 {
        ctx.set_zoom_factor(zoom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn back_becomes_escape_both_ways() {
        let mut raw = RawInput {
            events: vec![key(Key::BrowserBack, true), key(Key::BrowserBack, false)],
            ..Default::default()
        };

        back_is_escape(&mut raw);

        let all_escape = raw.events.iter().all(|event| {
            let egui::Event::Key { key, .. } = event else {
                return false;
            };

            *key == Key::Escape
        });
        assert!(all_escape);
    }
}
