//! TorrServer preload wait: live percent meter + backdrop screen.

use std::sync::{Arc, Mutex, MutexGuard};

use cinebox_core::i18n::Msg;
use cinebox_core::tmdb_image_url;
use cinebox_torrserver::PreloadEvent;
use egui::{Align2, CornerRadius, Rect, Ui, UiBuilder, pos2, vec2};
use egui_async::Bind;

use crate::images::ImageSlot;
use crate::screens::torrents::TorrentFileRow;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::backdrop;

const SCRIM_T: f32 = 0.75;
const BAR_H: f32 = 6.0;

/// Live preload progress, written by the wait job, read by the UI.
/// A smaller sibling of the settings screen's `SpeedMeter`.
#[derive(Clone)]
pub struct PreloadMeter {
    inner: Arc<Mutex<Live>>,
}

#[derive(Clone, Copy, Default)]
struct Live {
    percent: f64,
}

impl PreloadMeter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Live::default())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Live> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn on_event(&self, event: PreloadEvent) {
        let PreloadEvent::Progress { percent, .. } = event;

        let mut live = self.lock();
        live.percent = percent.clamp(0.0, 100.0);
    }

    #[must_use]
    pub fn percent(&self) -> f64 {
        self.lock().percent
    }
}

/// One in-flight preload wait plus everything needed to start playback after it.
pub struct Buffering {
    pub card: crate::screens::play::WatchCard,
    pub title: String,
    pub backdrop_path: Option<String>,
    pub hash: String,
    pub files: Vec<TorrentFileRow>,
    pub file_index: usize,
    pub resume_at: f64,
    pub meter: PreloadMeter,
    pub job: Bind<(), String>,
}

/// Backdrop + dimming scrim + centered title / percent bar, in place of the video.
pub fn paint(ui: &mut Ui, rect: Rect, svc: &Services, theme: &Theme, buffering: &Buffering) {
    ui.painter().rect_filled(rect, 0.0, theme.video_bg);

    let url = tmdb_image_url(buffering.backdrop_path.as_deref(), "w1280");
    if let ImageSlot::Ready(texture) = svc.images.backdrop(url.as_deref()) {
        let mut wall = ui.new_child(UiBuilder::new().max_rect(rect));
        backdrop::paint(&mut wall, texture, theme);
    }

    ui.painter()
        .rect_filled(rect, 0.0, theme.overlay_at(SCRIM_T));

    let percent = buffering.meter.percent();
    let center = rect.center();

    ui.painter().text(
        pos2(center.x, center.y - 64.0),
        Align2::CENTER_CENTER,
        &buffering.title,
        theme.title_font(theme.text_display),
        theme.title,
    );

    ui.painter().text(
        pos2(center.x, center.y - 24.0),
        Align2::CENTER_CENTER,
        Msg::Preloading.t(),
        theme.ui_font(theme.text_body),
        theme.muted_bright,
    );

    let bar_w = (rect.width() * 0.4)
        .clamp(240.0, 480.0)
        .min(rect.width() - 32.0);
    let bar = Rect::from_center_size(pos2(center.x, center.y + 8.0), vec2(bar_w, BAR_H));
    ui.painter()
        .rect_filled(bar, CornerRadius::same(3), theme.progress_track);

    let fraction = (percent / 100.0).clamp(0.0, 1.0) as f32;
    if fraction > 0.0 {
        let mut fill = bar;
        fill.max.x = bar.left() + bar.width() * fraction;
        ui.painter()
            .rect_filled(fill, CornerRadius::same(3), theme.progress_fill);
    }

    ui.painter().text(
        pos2(center.x, center.y + 40.0),
        Align2::CENTER_CENTER,
        format!("{percent:.0}%"),
        theme.ui_font(theme.text_section),
        theme.title,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_tracks_progress_events() {
        let meter = PreloadMeter::new();
        assert!(meter.percent().abs() < f64::EPSILON);

        meter.on_event(PreloadEvent::Progress {
            preloaded_bytes: 50,
            preload_size: 100,
            percent: 50.0,
        });

        assert!((meter.percent() - 50.0).abs() < f64::EPSILON);

        meter.on_event(PreloadEvent::Progress {
            preloaded_bytes: 500,
            preload_size: 100,
            percent: 500.0,
        });

        assert!((meter.percent() - 100.0).abs() < f64::EPSILON);
    }
}
