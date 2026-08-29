//! libmpv2 playback with a child HWND (`wid` before `loadfile`).
//!
//! Stream URLs come from TorrServer. Pass
//! `cinebox_torrserver::mpv_http_header_fields` as `PlayOpts::http_header_fields`
//! when Basic auth is set. Never log that value.

#![cfg_attr(not(windows), allow(unused_imports))]

mod engine;
mod error;
mod layout;

pub use engine::{Engine, PlayOpts, Snapshot};
pub use error::Error;
pub use layout::{
    ClickZone, FOOTER_LOGICAL, HEADER_LOGICAL, PixelRect, SEEK_SECS, click_zone, format_clock,
    video_rect, wid_from_hwnd,
};
