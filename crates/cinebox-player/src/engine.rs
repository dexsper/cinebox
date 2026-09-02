//! libmpv OpenGL render API. The only `unsafe` in the workspace.

use std::ffi::{CStr, CString, c_void};
use std::sync::Arc;

use cinebox_core::VideoScale;
use libmpv2::Mpv;
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use tracing::info;

use crate::error::Error;

/// eframe glow `get_proc_address` loader.
pub type GlLoader = Arc<dyn Fn(&CStr) -> *const c_void + Send + Sync>;

/// Options applied before `loadfile`.
#[derive(Clone, Copy)]
pub struct PlayOpts<'a> {
    pub http_header_fields: Option<&'a str>,
    pub loudnorm: bool,
    pub start_seconds: f64,
}

/// Polled playback snapshot for the player chrome.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub paused: bool,
    pub time: f64,
    pub duration: f64,
    pub eof: bool,
    pub aid: i64,
    pub sid: i64,
    pub volume: f64,
    pub muted: bool,
    pub speed: f64,
}

/// Stream kind from mpv `track-list/N/type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Subtitle,
}

impl TrackKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "video" => Some(Self::Video),
            "audio" => Some(Self::Audio),
            "sub" => Some(Self::Subtitle),
            _ => None,
        }
    }
}

/// One entry of mpv's `track-list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: i64,
    pub kind: TrackKind,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub selected: bool,
}

/// Embedded mpv session: OpenGL render context + player.
///
/// `render` is dropped before `mpv` (declaration order). The `'static` lifetime
/// on `RenderContext` is a self-referential borrow of `mpv`; `mpv` is boxed so
/// its address is stable across moves of `Engine`.
///
/// Used only on the eframe/glow UI thread (paint callback + controls).
pub struct Engine {
    render: Option<RenderContext<'static>>,
    mpv: Box<Mpv>,
}

// SAFETY: `Engine` is driven exclusively on the eframe/glow UI thread. The
// glow paint callback is `Send + Sync` so the handle must cross that bound,
// but mpv APIs are never called from another thread.
unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

impl Engine {
    /// Create libmpv with `vo=libmpv` and an OpenGL render context.
    ///
    /// The GL context must be current on this thread (eframe glow backend).
    ///
    /// # Errors
    ///
    /// Missing bundled libmpv, or render-context creation failure.
    pub fn attach(loader: GlLoader) -> Result<Self, Error> {
        let mpv = Box::new(
            Mpv::with_initializer(|init| {
                init.set_option("vo", "libmpv")?;
                init.set_option("video-timing-offset", 0i64)?;
                init.set_option("input-vo-keyboard", false)?;
                init.set_option("input-default-bindings", false)?;
                init.set_option("osc", false)?;
                init.set_option("osd-level", 1i64)?;
                Ok(())
            })
            .map_err(|_| Error::MpvInit)?,
        );
        let render = unsafe { create_render(&mpv, loader)? };
        info!("mpv render context attached");
        Ok(Self {
            render: Some(render),
            mpv,
        })
    }

    /// Notify the UI thread when a new video frame is ready.
    ///
    /// The callback must not call mpv APIs.
    pub fn set_update_callback<F: Fn() + Send + 'static>(&mut self, callback: F) {
        if let Some(render) = self.render.as_mut() {
            render.set_update_callback(callback);
        }
    }

    /// Draw the current frame into `fbo` (0 = default framebuffer).
    ///
    /// # Errors
    ///
    /// mpv render failure.
    pub fn render(&self, fbo: u32, width: i32, height: i32) -> Result<(), Error> {
        let Some(render) = self.render.as_ref() else {
            return Err(Error::MpvInit);
        };
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        render
            .render::<()>(fbo as i32, width, height, true)
            .map_err(Error::mpv)
    }

    /// Load (or replace) a stream URL. Never log `opts.http_header_fields`.
    ///
    /// # Errors
    ///
    /// mpv command/property failures.
    pub fn load(&self, url: &str, opts: PlayOpts<'_>) -> Result<(), Error> {
        apply_play_opts(&self.mpv, opts)?;

        let owned = loadfile_args(url, opts.start_seconds);
        let args: Vec<&str> = owned.iter().map(String::as_str).collect();

        self.mpv.command("loadfile", &args).map_err(Error::mpv)
    }

    /// Stop playback and clear the playlist.
    pub fn stop(&self) {
        let _ = self.mpv.command("stop", &[]);
    }

    /// Toggle pause. Returns the new paused flag.
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn toggle_pause(&self) -> Result<bool, Error> {
        let paused: bool = self.mpv.get_property("pause").map_err(Error::mpv)?;
        let next = !paused;
        self.mpv.set_property("pause", next).map_err(Error::mpv)?;
        Ok(next)
    }

    /// Relative seek in seconds.
    ///
    /// # Errors
    ///
    /// mpv command failure.
    pub fn seek(&self, delta: f64) -> Result<(), Error> {
        let amount = format!("{delta}");
        self.mpv
            .command("seek", &[&amount, "relative"])
            .map_err(Error::mpv)
    }

    /// Absolute seek in seconds (progress-bar scrubbing).
    ///
    /// # Errors
    ///
    /// mpv command failure.
    pub fn seek_abs(&self, seconds: f64) -> Result<(), Error> {
        let amount = format!("{seconds}");
        self.mpv
            .command("seek", &[&amount, "absolute"])
            .map_err(Error::mpv)
    }

    /// Apply a video-fit mode live (no reload needed).
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn set_scale(&self, scale: VideoScale) -> Result<(), Error> {
        apply_scale(&self.mpv, scale)
    }

    /// Playback speed multiplier (`1.0` = normal).
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn set_speed(&self, speed: f64) -> Result<(), Error> {
        self.mpv.set_property("speed", speed).map_err(Error::mpv)
    }

    /// Playback volume, `0.0..=100.0`.
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn set_volume(&self, volume: f64) -> Result<(), Error> {
        self.mpv.set_property("volume", volume).map_err(Error::mpv)
    }

    /// Mute without touching the volume level.
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn set_mute(&self, muted: bool) -> Result<(), Error> {
        self.mpv.set_property("mute", muted).map_err(Error::mpv)
    }

    /// Select an audio track by mpv id.
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn select_audio(&self, id: i64) -> Result<(), Error> {
        self.mpv.set_property("aid", id).map_err(Error::mpv)
    }

    /// Select a subtitle track by mpv id; `None` turns subtitles off.
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn select_sub(&self, id: Option<i64>) -> Result<(), Error> {
        let Some(id) = id else {
            return self
                .mpv
                .set_property("sid", "no".to_owned())
                .map_err(Error::mpv);
        };

        self.mpv.set_property("sid", id).map_err(Error::mpv)
    }

    /// Subtitle font scale multiplier (`1.0` = normal).
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn set_sub_scale(&self, scale: f64) -> Result<(), Error> {
        self.mpv
            .set_property("sub-scale", scale)
            .map_err(Error::mpv)
    }

    /// Subtitle delay in seconds.
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn set_sub_delay(&self, seconds: f64) -> Result<(), Error> {
        self.mpv
            .set_property("sub-delay", seconds)
            .map_err(Error::mpv)
    }

    /// Real track list from mpv's indexed `track-list/N/*` properties.
    #[must_use]
    pub fn track_list(&self) -> Vec<Track> {
        let count: i64 = self.mpv.get_property("track-list/count").unwrap_or(0);
        let mut tracks = Vec::new();

        for index in 0..count {
            let kind_raw: String = self
                .mpv
                .get_property(&format!("track-list/{index}/type"))
                .unwrap_or_default();

            let Some(kind) = TrackKind::parse(&kind_raw) else {
                continue;
            };

            let Ok(id) = self.mpv.get_property(&format!("track-list/{index}/id")) else {
                continue;
            };

            let lang: Option<String> = self
                .mpv
                .get_property(&format!("track-list/{index}/lang"))
                .ok();

            let title: Option<String> = self
                .mpv
                .get_property(&format!("track-list/{index}/title"))
                .ok();

            let selected: bool = self
                .mpv
                .get_property(&format!("track-list/{index}/selected"))
                .unwrap_or(false);

            tracks.push(Track {
                id,
                kind,
                lang,
                title,
                selected,
            });
        }

        tracks
    }

    /// Best-effort playback status.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            paused: self.mpv.get_property("pause").unwrap_or(false),
            time: self.mpv.get_property("time-pos").unwrap_or(0.0),
            duration: self.mpv.get_property("duration").unwrap_or(0.0),
            eof: self.mpv.get_property("eof-reached").unwrap_or(false),
            aid: self.mpv.get_property("aid").unwrap_or(0),
            sid: self.mpv.get_property("sid").unwrap_or(0),
            volume: self.mpv.get_property("volume").unwrap_or(100.0),
            muted: self.mpv.get_property("mute").unwrap_or(false),
            speed: self.mpv.get_property("speed").unwrap_or(1.0),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.render.take();
    }
}

unsafe fn create_render(mpv: &Mpv, loader: GlLoader) -> Result<RenderContext<'static>, Error> {
    let params = vec![
        RenderParam::ApiType(RenderParamApiType::OpenGl),
        RenderParam::InitParams(OpenGLInitParams {
            get_proc_address: load_gl,
            ctx: loader,
        }),
    ];
    let render = mpv.create_render_context(params).map_err(Error::mpv)?;
    // SAFETY: `Engine` boxes `Mpv` so its address is stable. `render` is stored
    // in a field that is dropped before `mpv`.
    Ok(unsafe { std::mem::transmute::<RenderContext<'_>, RenderContext<'static>>(render) })
}

fn load_gl(loader: &GlLoader, name: &str) -> *mut c_void {
    let Ok(cname) = CString::new(name) else {
        return std::ptr::null_mut();
    };
    loader(cname.as_c_str()) as *mut c_void
}

/// mpv 0.38+ parses the 3rd `loadfile` argument as playlist index, not options.
/// `start=12` there is `MPV_ERROR_INVALID_PARAMETER` (-4).
fn loadfile_args(url: &str, start_seconds: f64) -> Vec<String> {
    let start = start_seconds.max(0.0);
    if start > 0.5 {
        return vec![
            url.to_owned(),
            String::from("replace"),
            String::from("-1"),
            format!("start={start}"),
        ];
    }

    vec![url.to_owned(), String::from("replace")]
}

fn apply_play_opts(mpv: &Mpv, opts: PlayOpts<'_>) -> Result<(), Error> {
    if let Some(header) = opts.http_header_fields.filter(|h| !h.is_empty()) {
        mpv.set_property("http-header-fields", header.to_owned())
            .map_err(Error::mpv)?;
    }
    if opts.loudnorm {
        mpv.set_property("af", "loudnorm".to_owned())
            .map_err(Error::mpv)?;
    }
    Ok(())
}

fn apply_scale(mpv: &Mpv, scale: VideoScale) -> Result<(), Error> {
    let keep_aspect = scale != VideoScale::Fill;
    let panscan = if scale == VideoScale::Expand {
        1.0
    } else {
        0.0
    };

    let zoom = match scale {
        VideoScale::Zoom115 => 1.15f64.log2(),
        VideoScale::Zoom130 => 1.30f64.log2(),
        VideoScale::Default | VideoScale::Expand | VideoScale::Fill => 0.0,
    };

    mpv.set_property("keepaspect", keep_aspect)
        .map_err(Error::mpv)?;
    mpv.set_property("video-unscaled", false)
        .map_err(Error::mpv)?;
    mpv.set_property("panscan", panscan).map_err(Error::mpv)?;
    mpv.set_property("video-zoom", zoom).map_err(Error::mpv)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_kind_parses_mpv_names() {
        assert_eq!(TrackKind::parse("video"), Some(TrackKind::Video));
        assert_eq!(TrackKind::parse("audio"), Some(TrackKind::Audio));
        assert_eq!(TrackKind::parse("sub"), Some(TrackKind::Subtitle));
        assert_eq!(TrackKind::parse("unknown"), None);
    }

    #[test]
    fn loadfile_resume_uses_index_then_start_option() {
        let args = loadfile_args("http://ts/stream", 42.5);
        assert_eq!(
            args,
            vec![
                String::from("http://ts/stream"),
                String::from("replace"),
                String::from("-1"),
                String::from("start=42.5"),
            ]
        );
    }

    #[test]
    fn loadfile_without_resume_has_no_start_option() {
        let args = loadfile_args("http://ts/stream", 0.0);
        assert_eq!(
            args,
            vec![String::from("http://ts/stream"), String::from("replace")]
        );
    }
}
