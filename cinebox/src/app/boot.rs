//! What the app sets up once at start: the video player, language, search history.

use std::sync::Arc;

use cinebox_core::SEARCH_HISTORY_LIMIT;
use tracing::error;

use crate::services::{Services, db_block_on};

#[cfg(not(target_os = "android"))]
pub(super) fn attach_mpv(
    cc: &eframe::CreationContext<'_>,
) -> Option<Arc<dyn cinebox_player::Player>> {
    let loader = cc.get_proc_address.clone()?;
    match cinebox_player::MpvPlayer::attach(loader, native_display(cc)) {
        Ok(player) => Some(Arc::new(player)),
        Err(error) => {
            error!(%error, "mpv render attach failed");
            None
        }
    }
}

/// There is no libmpv on Android; the entry point always brings a player.
#[cfg(target_os = "android")]
pub(super) fn attach_mpv(
    _cc: &eframe::CreationContext<'_>,
) -> Option<Arc<dyn cinebox_player::Player>> {
    None
}

/// VAAPI shares decoded frames with GL only through the window's display.
/// The display belongs to winit's event loop, which outlives the app and its engine.
#[cfg(target_os = "linux")]
fn native_display(cc: &eframe::CreationContext<'_>) -> cinebox_player::NativeDisplay {
    use cinebox_player::NativeDisplay;
    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

    let Ok(handle) = cc.display_handle() else {
        return NativeDisplay::None;
    };

    match handle.as_raw() {
        RawDisplayHandle::Wayland(wayland) => NativeDisplay::Wayland(wayland.display),
        RawDisplayHandle::Xlib(xlib) => {
            xlib.display.map_or(NativeDisplay::None, NativeDisplay::X11)
        }
        _ => NativeDisplay::None,
    }
}

/// Windows decoders (NVDEC, D3D11VA copy-back) need no display.
#[cfg(target_os = "windows")]
fn native_display(_cc: &eframe::CreationContext<'_>) -> cinebox_player::NativeDisplay {
    cinebox_player::NativeDisplay::None
}

/// A fresh install starts in the OS language when there is a translation for it.
pub(super) fn adopt_system_language(services: &mut Services) {
    let Some(language) = crate::i18n::system_language() else {
        return;
    };

    services.settings.general.language = language;
}

pub(super) fn load_search_history(svc: &Services) -> Vec<String> {
    let Some(db) = &svc.db else {
        return Vec::new();
    };

    db_block_on(db.recent_searches(SEARCH_HISTORY_LIMIT)).unwrap_or_default()
}
