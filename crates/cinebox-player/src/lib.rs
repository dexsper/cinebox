//! libmpv2 playback via the OpenGL render API (`vo=libmpv`).

mod engine;
mod error;
mod layout;

pub use engine::{Engine, GlLoader, NativeDisplay, PlayOpts, Snapshot, Track, TrackKind, VideoDecoder};
pub use error::Error;
pub use layout::{ClickZone, SEEK_SECS, click_zone, format_clock};
