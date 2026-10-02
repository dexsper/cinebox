//! libmpv2 playback via the OpenGL render API (`vo=libmpv`).

#[cfg(target_os = "android")]
mod android;
mod engine;
mod error;
mod layout;

pub use engine::{Engine, GlLoader, NativeDisplay, PlayOpts, Snapshot, Track, TrackKind, VideoDecoder};
#[cfg(target_os = "android")]
pub use android::init_mediacodec;
pub use error::Error;
pub use layout::{ClickZone, SEEK_SECS, click_zone, format_clock};
