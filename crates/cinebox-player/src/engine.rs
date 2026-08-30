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
    pub scale: VideoScale,
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
        let start = opts.start_seconds.max(0.0);
        if start > 0.5 {
            let extra = format!("start={start}");
            return self
                .mpv
                .command("loadfile", &[url, "replace", &extra])
                .map_err(Error::mpv);
        }
        self.mpv
            .command("loadfile", &[url, "replace"])
            .map_err(Error::mpv)
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

    /// Cycle audio tracks.
    ///
    /// # Errors
    ///
    /// mpv command failure.
    pub fn cycle_audio(&self) -> Result<(), Error> {
        self.mpv.command("cycle", &["aid"]).map_err(Error::mpv)
    }

    /// Cycle subtitle tracks (includes “no”).
    ///
    /// # Errors
    ///
    /// mpv command failure.
    pub fn cycle_subs(&self) -> Result<(), Error> {
        self.mpv.command("cycle", &["sid"]).map_err(Error::mpv)
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

fn apply_play_opts(mpv: &Mpv, opts: PlayOpts<'_>) -> Result<(), Error> {
    if let Some(header) = opts.http_header_fields.filter(|h| !h.is_empty()) {
        mpv.set_property("http-header-fields", header.to_owned())
            .map_err(Error::mpv)?;
    }
    if opts.loudnorm {
        mpv.set_property("af", "loudnorm".to_owned())
            .map_err(Error::mpv)?;
    }
    apply_scale(mpv, opts.scale)
}

fn apply_scale(mpv: &Mpv, scale: VideoScale) -> Result<(), Error> {
    match scale {
        VideoScale::KeepAspect => {
            mpv.set_property("keepaspect", true).map_err(Error::mpv)?;
            mpv.set_property("video-unscaled", false)
                .map_err(Error::mpv)?;
            mpv.set_property("panscan", 0.0f64).map_err(Error::mpv)?;
        }
        VideoScale::Unscaled => {
            mpv.set_property("video-unscaled", true)
                .map_err(Error::mpv)?;
        }
        VideoScale::Panscan => {
            mpv.set_property("keepaspect", true).map_err(Error::mpv)?;
            mpv.set_property("video-unscaled", false)
                .map_err(Error::mpv)?;
            mpv.set_property("panscan", 1.0f64).map_err(Error::mpv)?;
        }
    }
    Ok(())
}
