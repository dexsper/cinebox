//! Desktop window vs. a TV driven by a remote.
//!
//! A runtime flag rather than only `cfg`, so desktop tests can drive the TV layout.

use std::sync::Arc;

use egui::{Context, Id};

/// How the app is shown and operated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Form {
    /// Own window chrome, mouse first.
    #[default]
    Desktop,
    /// Always fullscreen, operated with a D-pad remote.
    Tv,
}

/// Keeps the display from sleeping while `true`.
pub type KeepAwake = Arc<dyn Fn(bool) + Send + Sync>;

/// What the platform entry point hands the shared app.
#[derive(Clone, Default)]
pub struct Host {
    pub form: Form,
    pub keep_awake: Option<KeepAwake>,
}

impl Host {
    #[must_use]
    pub fn tv(keep_awake: KeepAwake) -> Self {
        Self {
            form: Form::Tv,
            keep_awake: Some(keep_awake),
        }
    }
}

fn host_id() -> Id {
    Id::new("cinebox-host")
}

pub fn set(ctx: &Context, host: Host) {
    ctx.data_mut(|data| data.insert_temp(host_id(), host));
}

fn host(ctx: &Context) -> Host {
    ctx.data(|data| data.get_temp::<Host>(host_id()))
        .unwrap_or_default()
}

#[must_use]
pub fn is_tv(ctx: &Context) -> bool {
    host(ctx).form == Form::Tv
}

/// Playback keeps the screen on; leaving the player lets it sleep again.
pub fn keep_awake(ctx: &Context, on: bool) {
    let Some(keep_awake) = host(ctx).keep_awake else {
        return;
    };

    keep_awake(on);
}
