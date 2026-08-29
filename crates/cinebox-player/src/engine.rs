//! Child HWND + libmpv. The only `unsafe` in the workspace.

use cinebox_core::VideoScale;
use libmpv2::Mpv;
use tracing::info;

use crate::error::Error;
use crate::layout;

/// Options applied before `loadfile`.
#[derive(Clone, Copy)]
pub struct PlayOpts<'a> {
    pub http_header_fields: Option<&'a str>,
    pub loudnorm: bool,
    pub scale: VideoScale,
    pub start_seconds: f64,
}

/// Polled playback snapshot for the Iced chrome.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub paused: bool,
    pub time: f64,
    pub duration: f64,
    pub eof: bool,
    pub aid: i64,
    pub sid: i64,
}

/// Embedded mpv session: child hole + player.
pub struct Engine {
    parent: isize,
    child: isize,
    mpv: Option<Mpv>,
}

impl Engine {
    /// Create a click-through child of `parent` and start libmpv with `wid`.
    ///
    /// # Errors
    ///
    /// Missing bundled libmpv, or the child window cannot be created.
    pub fn attach(parent: isize) -> Result<Self, Error> {
        let child = create_hole(parent)?;
        layout_hole(parent, child);
        let mpv = start_mpv(child)?;
        info!("mpv attached");
        Ok(Self {
            parent,
            child,
            mpv: Some(mpv),
        })
    }

    /// Load (or replace) a stream URL. Never log `opts.http_header_fields`.
    ///
    /// # Errors
    ///
    /// mpv command/property failures.
    pub fn load(&self, url: &str, opts: PlayOpts<'_>) -> Result<(), Error> {
        let Some(mpv) = self.mpv.as_ref() else {
            return Err(Error::MpvInit);
        };
        apply_play_opts(mpv, opts)?;
        let start = opts.start_seconds.max(0.0);
        if start > 0.5 {
            let extra = format!("start={start}");
            return mpv
                .command("loadfile", &[url, "replace", &extra])
                .map_err(Error::mpv);
        }
        mpv.command("loadfile", &[url, "replace"])
            .map_err(Error::mpv)
    }

    /// Keep the hole aligned after the parent client size changes.
    pub fn relayout(&self) {
        layout_hole(self.parent, self.child);
    }

    /// Toggle pause. Returns the new paused flag.
    ///
    /// # Errors
    ///
    /// mpv property failures.
    pub fn toggle_pause(&self) -> Result<bool, Error> {
        let mpv = self.mpv.as_ref().ok_or(Error::MpvInit)?;
        let paused: bool = mpv.get_property("pause").unwrap_or(false);
        let next = !paused;
        mpv.set_property("pause", next).map_err(Error::mpv)?;
        Ok(next)
    }

    /// Relative seek in seconds.
    ///
    /// # Errors
    ///
    /// mpv command failure.
    pub fn seek(&self, delta: f64) -> Result<(), Error> {
        let mpv = self.mpv.as_ref().ok_or(Error::MpvInit)?;
        let amount = format!("{delta}");
        mpv.command("seek", &[&amount, "relative"])
            .map_err(Error::mpv)
    }

    /// Cycle audio tracks.
    ///
    /// # Errors
    ///
    /// mpv command failure.
    pub fn cycle_audio(&self) -> Result<(), Error> {
        let mpv = self.mpv.as_ref().ok_or(Error::MpvInit)?;
        mpv.command("cycle", &["aid"]).map_err(Error::mpv)
    }

    /// Cycle subtitle tracks (includes “no”).
    ///
    /// # Errors
    ///
    /// mpv command failure.
    pub fn cycle_subs(&self) -> Result<(), Error> {
        let mpv = self.mpv.as_ref().ok_or(Error::MpvInit)?;
        mpv.command("cycle", &["sid"]).map_err(Error::mpv)
    }

    /// Best-effort playback status.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let Some(mpv) = self.mpv.as_ref() else {
            return Snapshot {
                paused: true,
                time: 0.0,
                duration: 0.0,
                eof: false,
                aid: 0,
                sid: 0,
            };
        };
        Snapshot {
            paused: mpv.get_property("pause").unwrap_or(false),
            time: mpv.get_property("time-pos").unwrap_or(0.0),
            duration: mpv.get_property("duration").unwrap_or(0.0),
            eof: mpv.get_property("eof-reached").unwrap_or(false),
            aid: mpv.get_property("aid").unwrap_or(0),
            sid: mpv.get_property("sid").unwrap_or(0),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.mpv.take();
        destroy_hole(self.child);
    }
}

fn start_mpv(child: isize) -> Result<Mpv, Error> {
    let wid = layout::wid_from_hwnd(child);
    Mpv::with_initializer(|init| {
        init.set_option("wid", wid)?;
        init.set_option("input-vo-keyboard", false)?;
        init.set_option("input-default-bindings", false)?;
        init.set_option("osc", false)?;
        init.set_option("osd-level", 1i64)?;
        Ok(())
    })
    .map_err(|_| Error::MpvInit)
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

#[cfg(windows)]
fn create_hole(parent: isize) -> Result<isize, Error> {
    hole::create(parent)
}

#[cfg(not(windows))]
fn create_hole(_parent: isize) -> Result<isize, Error> {
    Err(Error::Unsupported)
}

#[cfg(windows)]
fn layout_hole(parent: isize, child: isize) {
    hole::layout(parent, child);
}

#[cfg(not(windows))]
fn layout_hole(_parent: isize, _child: isize) {}

#[cfg(windows)]
fn destroy_hole(child: isize) {
    hole::destroy(child);
}

#[cfg(not(windows))]
fn destroy_hole(_child: isize) {}

#[cfg(windows)]
mod hole {
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{COLOR_WINDOW, HBRUSH};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, RegisterClassW,
        SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS,
        WS_EX_NOACTIVATE, WS_EX_TRANSPARENT, WS_VISIBLE,
    };

    use super::Error;
    use crate::layout::video_rect;

    const CLASS: &[u16] = &[
        b'C' as u16,
        b'i' as u16,
        b'n' as u16,
        b'e' as u16,
        b'b' as u16,
        b'o' as u16,
        b'x' as u16,
        b'M' as u16,
        b'p' as u16,
        b'v' as u16,
        0,
    ];

    static CLASS_ONCE: OnceLock<bool> = OnceLock::new();

    pub(super) fn create(parent: isize) -> Result<isize, Error> {
        register()?;
        let parent_hwnd = parent as HWND;
        // SAFETY: a null module name returns this process's HINSTANCE.
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        // SAFETY: `parent` is the Iced Win32 HWND. WS_EX_TRANSPARENT lets Iced
        // receive clicks on the hole while mpv still paints into this child.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
                CLASS.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                0,
                0,
                1,
                1,
                parent_hwnd,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(Error::Hole);
        }
        Ok(hwnd as isize)
    }

    pub(super) fn layout(parent: isize, child: isize) {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let parent_hwnd = parent as HWND;
        let child_hwnd = child as HWND;
        // SAFETY: both HWNDs are live while Engine exists.
        let ok = unsafe { GetClientRect(parent_hwnd, &mut rect) };
        if ok == 0 {
            return;
        }
        let dpi = unsafe { GetDpiForWindow(parent_hwnd) };
        let hole = video_rect(rect.right - rect.left, rect.bottom - rect.top, dpi);
        // SAFETY: child HWND is still owned by Engine.
        unsafe {
            let _ = SetWindowPos(
                child_hwnd,
                std::ptr::null_mut(),
                hole.x,
                hole.y,
                hole.w,
                hole.h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    pub(super) fn destroy(child: isize) {
        if child == 0 {
            return;
        }
        // SAFETY: child was created by `create`; mpv is already dropped.
        unsafe {
            let _ = DestroyWindow(child as HWND);
        }
    }

    fn register() -> Result<(), Error> {
        CLASS_ONCE.get_or_init(|| {
            // SAFETY: a null module name returns this process's HINSTANCE.
            let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
            let class = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
                lpszMenuName: std::ptr::null(),
                lpszClassName: CLASS.as_ptr(),
            };
            // SAFETY: class name is a static NUL-terminated UTF-16 string.
            unsafe {
                let _ = RegisterClassW(&class);
            }
            true
        });
        Ok(())
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // SAFETY: default processing for a hole that does not paint.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}
