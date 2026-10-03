//! Video playback behind one interface ([`Player`]): libmpv drawn into the
//! window's GL context on the desktop, or the player a platform brings.

mod error;
mod fit;
mod layout;
#[cfg(not(target_os = "android"))]
mod mpv;
mod player;

pub use error::Error;
pub use fit::{Area, PixelRect, fit_video};
pub use layout::{ClickZone, SEEK_SECS, click_zone, format_clock};
#[cfg(not(target_os = "android"))]
pub use mpv::{GlLoader, MpvPlayer, NativeDisplay};
pub use player::{
    Failure, Features, Media, Player, Snapshot, Track, TrackKind, VideoDecoder, VideoOutput,
};
