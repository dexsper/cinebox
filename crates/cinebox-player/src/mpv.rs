//! libmpv through its OpenGL render API (`vo=libmpv`), drawn into the window's
//! GL context. The only `unsafe` in the crate: a `Send` impl (all access goes
//! through the mutex) and the self-referential render-context setup.

use std::ffi::{CStr, CString, c_void};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, MutexGuard};

use cinebox_core::VideoScale;
use libmpv2::Mpv;
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use tracing::{info, warn};

use crate::error::Error;
use crate::player::{
    Features, Media, Player, Snapshot, Track, TrackKind, VideoDecoder, VideoOutput,
};

/// eframe glow `get_proc_address` loader.
pub type GlLoader = Arc<dyn Fn(&CStr) -> *const c_void + Send + Sync>;

/// The window's display connection, for hardware decoding without copies.
///
/// VAAPI needs it to share decoded frames with the GL context; without it mpv
/// falls back to a copying decoder. The pointer must stay valid while the
/// [`MpvPlayer`] lives.
#[derive(Debug, Clone, Copy)]
pub enum NativeDisplay {
    Wayland(NonNull<c_void>),
    X11(NonNull<c_void>),
    None,
}

/// mpv drawing into the window's GL context. Used only on the eframe/glow UI
/// thread (paint callback + controls).
pub struct MpvPlayer {
    engine: Mutex<Engine>,
}

/// `render` is dropped before `mpv` (declaration order). The `'static`
/// lifetime on `RenderContext` is a self-referential borrow of `mpv`; `mpv` is
/// boxed so its address is stable across moves.
struct Engine {
    render: Option<RenderContext<'static>>,
    mpv: Box<Mpv>,
}

// SAFETY: `Engine` is driven exclusively on the eframe/glow UI thread, but the
// glow paint callback closure must be `Send + Sync`, so the `MpvPlayer` it
// captures has to cross that bound. `Mutex<Engine>: Sync` only needs
// `Engine: Send`; all access goes through the mutex, so mpv APIs are never
// called concurrently.
unsafe impl Send for Engine {}

impl Drop for Engine {
    fn drop(&mut self) {
        self.render.take();
    }
}

impl MpvPlayer {
    /// Create libmpv with `vo=libmpv` and an OpenGL render context.
    ///
    /// The GL context must be current on this thread (eframe glow backend).
    ///
    /// # Errors
    ///
    /// Missing bundled libmpv, or render-context creation failure.
    pub fn attach(loader: GlLoader, display: NativeDisplay) -> Result<Self, Error> {
        let mpv = Box::new(
            Mpv::with_initializer(|init| {
                init.set_option("vo", "libmpv")?;
                init.set_option("video-timing-offset", 0i64)?;
                init.set_option("input-vo-keyboard", false)?;
                init.set_option("input-default-bindings", false)?;
                init.set_option("osd-level", 1i64)?;

                match init.set_option("osc", false) {
                    Ok(()) | Err(libmpv2::Error::Raw(libmpv2::mpv_error::OptionNotFound)) => {}
                    Err(error) => return Err(error),
                }

                Ok(())
            })
            .map_err(|_| Error::MpvInit)?,
        );
        let render = unsafe { create_render(&mpv, loader, display)? };
        info!("mpv render context attached");

        let engine = Engine {
            render: Some(render),
            mpv,
        };
        Ok(Self {
            engine: Mutex::new(engine),
        })
    }

    fn engine(&self) -> MutexGuard<'_, Engine> {
        self.engine
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn set<T: libmpv2::SetData>(&self, name: &str, value: T) -> Result<(), Error> {
        self.engine()
            .mpv
            .set_property(name, value)
            .map_err(|error| Error::mpv_prop(name, error))
    }

    fn command(&self, name: &str, args: &[&str]) -> Result<(), Error> {
        self.engine().mpv.command(name, args).map_err(Error::mpv)
    }
}

impl Player for MpvPlayer {
    fn output(&self) -> VideoOutput {
        VideoOutput::Rendered
    }

    fn features(&self) -> Features {
        Features::ALL
    }

    /// mpv calls back for every new frame, from its own thread.
    fn on_change(&self, wake: Box<dyn Fn() + Send + Sync>) {
        let mut engine = self.engine();
        if let Some(render) = engine.render.as_mut() {
            render.set_update_callback(wake);
        }
    }

    fn load(&self, media: &Media<'_>) -> Result<(), Error> {
        let engine = self.engine();
        apply_play_opts(&engine.mpv, media)?;

        let owned = loadfile_args(media.url, media.start_seconds);
        let args: Vec<&str> = owned.iter().map(String::as_str).collect();

        engine.mpv.command("loadfile", &args).map_err(Error::mpv)
    }

    fn stop(&self) {
        let _ = self.command("stop", &[]);
    }

    fn set_paused(&self, paused: bool) -> Result<(), Error> {
        self.set("pause", paused)
    }

    fn seek_by(&self, seconds: f64) -> Result<(), Error> {
        let amount = format!("{seconds}");
        self.command("seek", &[&amount, "relative"])
    }

    fn seek_to(&self, seconds: f64) -> Result<(), Error> {
        let amount = format!("{seconds}");
        self.command("seek", &[&amount, "absolute"])
    }

    fn set_scale(&self, scale: VideoScale) -> Result<(), Error> {
        let params = scale_params(scale);

        self.set("keepaspect", params.keep_aspect)?;
        self.set("video-unscaled", false)?;
        self.set("panscan", params.panscan)?;
        self.set("video-zoom", params.zoom)
    }

    fn set_speed(&self, speed: f64) -> Result<(), Error> {
        self.set("speed", speed)
    }

    fn set_volume(&self, volume: f64) -> Result<(), Error> {
        self.set("volume", volume)
    }

    fn set_muted(&self, muted: bool) -> Result<(), Error> {
        self.set("mute", muted)
    }

    fn select_audio(&self, id: i64) -> Result<(), Error> {
        self.set("aid", id)
    }

    fn select_subtitle(&self, id: Option<i64>) -> Result<(), Error> {
        let Some(id) = id else {
            return self.set("sid", "no".to_owned());
        };

        self.set("sid", id)
    }

    fn set_subtitle_scale(&self, scale: f64) -> Result<(), Error> {
        self.set("sub-scale", scale)
    }

    fn set_subtitle_delay(&self, seconds: f64) -> Result<(), Error> {
        self.set("sub-delay", seconds)
    }

    /// Real track list from mpv's indexed `track-list/N/*` properties.
    fn tracks(&self) -> Vec<Track> {
        let engine = self.engine();
        let mpv = &engine.mpv;
        let count: i64 = mpv.get_property("track-list/count").unwrap_or(0);
        let mut tracks = Vec::new();

        for index in 0..count {
            let kind_raw: String = mpv
                .get_property(&format!("track-list/{index}/type"))
                .unwrap_or_default();

            let Some(kind) = track_kind(&kind_raw) else {
                continue;
            };

            let Ok(id) = mpv.get_property(&format!("track-list/{index}/id")) else {
                continue;
            };

            let lang: Option<String> = mpv.get_property(&format!("track-list/{index}/lang")).ok();
            let title: Option<String> = mpv.get_property(&format!("track-list/{index}/title")).ok();
            let selected: bool = mpv
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

    fn snapshot(&self) -> Snapshot {
        let engine = self.engine();
        let mpv = &engine.mpv;
        let width: i64 = mpv.get_property("dwidth").unwrap_or(0);
        let height: i64 = mpv.get_property("dheight").unwrap_or(0);

        Snapshot {
            paused: mpv.get_property("pause").unwrap_or(false),
            time: mpv.get_property("time-pos").unwrap_or(0.0),
            duration: mpv.get_property("duration").unwrap_or(0.0),
            eof: mpv.get_property("eof-reached").unwrap_or(false),
            volume: mpv.get_property("volume").unwrap_or(100.0),
            muted: mpv.get_property("mute").unwrap_or(false),
            video_size: display_size(width, height),
            subtitle: None,
            failure: None,
        }
    }

    fn video_decoder(&self) -> Option<VideoDecoder> {
        let engine = self.engine();
        let text = |name: &str| {
            engine
                .mpv
                .get_property::<String>(name)
                .ok()
                .filter(|value| !value.is_empty())
        };

        Some(VideoDecoder {
            hwdec: text("hwdec-current")?,
            codec: text("video-format").unwrap_or_default(),
            // Hardware frames report the surface type in `pixelformat` and the real format here.
            pixel_format: text("video-params/hw-pixelformat")
                .or_else(|| text("video-params/pixelformat"))
                .unwrap_or_default(),
        })
    }

    fn render(&self, fbo: u32, width: i32, height: i32) -> Result<(), Error> {
        let engine = self.engine();
        let Some(render) = engine.render.as_ref() else {
            return Err(Error::MpvInit);
        };
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        render
            .render::<()>(fbo as i32, width, height, true)
            .map_err(Error::mpv)
    }
}

unsafe fn create_render(
    mpv: &Mpv,
    loader: GlLoader,
    display: NativeDisplay,
) -> Result<RenderContext<'static>, Error> {
    let mut params = vec![
        RenderParam::ApiType(RenderParamApiType::OpenGl),
        RenderParam::InitParams(OpenGLInitParams {
            get_proc_address: load_gl,
            ctx: loader,
        }),
    ];

    match display {
        NativeDisplay::Wayland(ptr) => params.push(RenderParam::WaylandDisplay(ptr.as_ptr())),
        NativeDisplay::X11(ptr) => params.push(RenderParam::X11Display(ptr.as_ptr())),
        NativeDisplay::None => {}
    }
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

/// Stream kind from mpv `track-list/N/type`.
fn track_kind(raw: &str) -> Option<TrackKind> {
    match raw {
        "video" => Some(TrackKind::Video),
        "audio" => Some(TrackKind::Audio),
        "sub" => Some(TrackKind::Subtitle),
        _ => None,
    }
}

fn display_size(width: i64, height: i64) -> Option<[u32; 2]> {
    let width = u32::try_from(width).ok().filter(|w| *w > 0)?;
    let height = u32::try_from(height).ok().filter(|h| *h > 0)?;

    Some([width, height])
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

/// Header fields, external audio, proxy and `af` are always written (an empty
/// string clears them) so one load cannot inherit another's.
fn apply_play_opts(mpv: &Mpv, media: &Media<'_>) -> Result<(), Error> {
    set_prop(mpv, "http-header-fields", media.headers.join(","))?;

    let audio = media.audio_url.unwrap_or("");
    set_prop(mpv, "audio-files", audio.to_owned())?;

    let proxy = media.http_proxy.unwrap_or("");
    set_prop(mpv, "http-proxy", proxy.to_owned())?;

    let hwdec = if media.hardware_decoding { "auto" } else { "no" };
    set_prop(mpv, "hwdec", hwdec.to_owned())?;

    if media.loudnorm {
        set_prop(mpv, "af", "loudnorm".to_owned())?;
        return Ok(());
    }

    set_prop(mpv, "af", String::new())?;
    Ok(())
}

fn set_prop(mpv: &Mpv, name: &'static str, value: String) -> Result<(), Error> {
    if let Err(error) = mpv.set_property(name, value) {
        warn!(%error, prop = name, "mpv set_property failed");
        return Err(Error::mpv_prop(name, error));
    }

    Ok(())
}

/// mpv property values for one [`VideoScale`] mode.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ScaleParams {
    keep_aspect: bool,
    panscan: f64,
    zoom: f64,
}

fn scale_params(scale: VideoScale) -> ScaleParams {
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

    ScaleParams {
        keep_aspect,
        panscan,
        zoom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_kind_parses_mpv_names() {
        assert_eq!(track_kind("video"), Some(TrackKind::Video));
        assert_eq!(track_kind("audio"), Some(TrackKind::Audio));
        assert_eq!(track_kind("sub"), Some(TrackKind::Subtitle));
        assert_eq!(track_kind("unknown"), None);
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

    #[test]
    fn loadfile_half_second_threshold() {
        assert_eq!(loadfile_args("u", 0.5).len(), 2);
        assert_eq!(loadfile_args("u", -3.0).len(), 2);

        let args = loadfile_args("u", 0.51);
        assert_eq!(args.len(), 4);
        assert_eq!(args[3], "start=0.51");
    }

    #[test]
    fn display_size_needs_both_sides() {
        assert_eq!(display_size(1920, 803), Some([1920, 803]));
        assert_eq!(display_size(0, 803), None);
        assert_eq!(display_size(1920, -1), None);
    }

    #[test]
    fn scale_modes_map_to_mpv_properties() {
        let default = scale_params(VideoScale::Default);
        assert!(default.keep_aspect);
        assert!((default.panscan - 0.0).abs() < f64::EPSILON);
        assert!((default.zoom - 0.0).abs() < f64::EPSILON);

        let fill = scale_params(VideoScale::Fill);
        assert!(!fill.keep_aspect);

        let expand = scale_params(VideoScale::Expand);
        assert!(expand.keep_aspect);
        assert!((expand.panscan - 1.0).abs() < f64::EPSILON);

        let zoom = scale_params(VideoScale::Zoom115);
        assert!((zoom.zoom - 1.15f64.log2()).abs() < f64::EPSILON);
        assert!((scale_params(VideoScale::Zoom130).zoom - 1.30f64.log2()).abs() < f64::EPSILON);
    }
}
