//! libmpv2 playback via the OpenGL render API (`vo=libmpv`).
//!
//! Stream URLs come from TorrServer. Pass
//! `cinebox_torrserver::mpv_http_header_fields` as `PlayOpts::http_header_fields`
//! when Basic auth is set. Never log that value.

mod engine;
mod error;
mod layout;

pub use engine::{Engine, GlLoader, PlayOpts, Snapshot};
pub use error::Error;
pub use layout::{
    ClickZone, FOOTER_LOGICAL, HEADER_LOGICAL, PixelRect, SEEK_SECS, click_zone, format_clock,
    video_rect,
};
