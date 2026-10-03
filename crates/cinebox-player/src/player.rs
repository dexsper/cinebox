//! What the app asks of a video player, whichever one the platform has.

use cinebox_core::VideoScale;

use crate::error::Error;
use crate::fit::Placement;

/// One stream to play. Never log `headers`: they may carry a password.
#[derive(Clone, Copy)]
pub struct Media<'a> {
    pub url: &'a str,
    /// `Name: value` lines sent with every request.
    pub headers: &'a [String],
    /// A separate audio stream played along (YouTube serves the two apart).
    pub audio_url: Option<&'a str>,
    pub http_proxy: Option<&'a str>,
    pub start_seconds: f64,
    pub loudnorm: bool,
    pub hardware_decoding: bool,
}

/// How the picture reaches the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoOutput {
    /// The app draws every frame into its GL framebuffer through [`Player::render`].
    Rendered,
    /// The platform shows the picture in a layer below the window, where
    /// [`Player::place_video`] puts it. The app fits the picture itself
    /// ([`crate::fit_video`], [`crate::place_in`]) and keeps the window clear over it.
    Underlay,
}

/// What the player lets the user change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Features {
    /// Volume and mute inside the app. Without it the OS or the receiver sets the level.
    pub volume: bool,
    pub speed: bool,
    pub subtitle_delay: bool,
    /// Subtitles are drawn into the picture; otherwise the app draws [`Snapshot::subtitle`].
    pub draws_subtitles: bool,
    pub loudnorm: bool,
    /// Hardware decoding can be turned off.
    pub decoder_choice: bool,
}

impl Features {
    pub const ALL: Self = Self {
        volume: true,
        speed: true,
        subtitle_delay: true,
        draws_subtitles: true,
        loudnorm: true,
        decoder_choice: true,
    };
}

/// Why playback stopped after a successful load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The container or a codec cannot be played on this device.
    Unsupported,
    /// The stream could not be read.
    Network,
    /// Anything else, in the player's own words (for the log).
    Other(String),
}

/// Polled playback state for the player chrome.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub paused: bool,
    pub time: f64,
    pub duration: f64,
    pub eof: bool,
    pub volume: f64,
    pub muted: bool,
    /// The picture's display size (pixel aspect applied); `None` until it is known.
    pub video_size: Option<[u32; 2]>,
    /// On screen now, for the app to draw (see [`Features::draws_subtitles`]).
    pub subtitle: Option<String>,
    pub failure: Option<Failure>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            paused: false,
            time: 0.0,
            duration: 0.0,
            eof: false,
            volume: 100.0,
            muted: false,
            video_size: None,
            subtitle: None,
            failure: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Subtitle,
}

/// One stream of the loaded file. `id` counts from 1 within its kind, in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: i64,
    pub kind: TrackKind,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub selected: bool,
}

/// Which decoder plays the video, for logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDecoder {
    /// `no` for software, otherwise e.g. `vaapi`, `d3d11va-copy`, or a MediaCodec name.
    pub hwdec: String,
    /// e.g. `hevc`.
    pub codec: String,
    /// e.g. `p010` for 10-bit.
    pub pixel_format: String,
}

/// A video player. Methods are called on the UI thread, except that the
/// callback given to [`Player::on_change`] may run on any thread.
pub trait Player: Send + Sync {
    fn output(&self) -> VideoOutput;

    fn features(&self) -> Features;

    /// `wake` runs when there is something new to show; it must not call the player.
    fn on_change(&self, wake: Box<dyn Fn() + Send + Sync>);

    /// Replace whatever is playing.
    ///
    /// # Errors
    ///
    /// The player refused the stream or its options.
    fn load(&self, media: &Media<'_>) -> Result<(), Error>;

    fn stop(&self);

    fn set_paused(&self, paused: bool) -> Result<(), Error>;

    fn seek_by(&self, seconds: f64) -> Result<(), Error>;

    fn seek_to(&self, seconds: f64) -> Result<(), Error>;

    /// Only [`VideoOutput::Rendered`] players scale the picture themselves.
    fn set_scale(&self, scale: VideoScale) -> Result<(), Error>;

    fn set_speed(&self, speed: f64) -> Result<(), Error>;

    /// `0.0..=100.0`.
    fn set_volume(&self, volume: f64) -> Result<(), Error>;

    fn set_muted(&self, muted: bool) -> Result<(), Error>;

    /// Takes effect once the file's tracks are known, if they are not yet.
    fn select_audio(&self, id: i64) -> Result<(), Error>;

    /// `None` turns subtitles off. Takes effect once the tracks are known.
    fn select_subtitle(&self, id: Option<i64>) -> Result<(), Error>;

    /// `1.0` is the normal size.
    fn set_subtitle_scale(&self, scale: f64) -> Result<(), Error>;

    fn set_subtitle_delay(&self, seconds: f64) -> Result<(), Error>;

    fn tracks(&self) -> Vec<Track>;

    fn snapshot(&self) -> Snapshot;

    /// `None` until the first frame is decoded.
    fn video_decoder(&self) -> Option<VideoDecoder>;

    /// [`VideoOutput::Rendered`]: draw the current frame into `fbo` (0 = default framebuffer).
    ///
    /// # Errors
    ///
    /// The renderer failed.
    fn render(&self, _fbo: u32, _width: i32, _height: i32) -> Result<(), Error> {
        Ok(())
    }

    /// [`VideoOutput::Underlay`]: where the picture goes.
    fn place_video(&self, _placement: Placement) {}
}
