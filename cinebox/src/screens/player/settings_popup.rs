//! In-player Settings flyout: nested menu for video size, speed, subtitles, audio.

use cinebox_core::TorrentPlaybackPrefs;
use cinebox_core::VideoScale;
use cinebox_core::i18n::Msg;
use cinebox_player::{Track, TrackKind};
use egui::{Align, CursorIcon, Layout, RichText, Sense, Ui, UiBuilder, vec2};
use egui_material_icons::icons::{ICON_ARROW_BACK, ICON_CHECK, ICON_CHEVRON_RIGHT};

use crate::theme::Theme;

const ROW_H: f32 = 36.0;
const STEPPER_BTN: f32 = 26.0;

pub const SPEEDS: &[f64] = &[0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

pub const SUB_SCALE_STEP: f64 = 0.1;
pub const SUB_SCALE_MIN: f64 = 0.5;
pub const SUB_SCALE_MAX: f64 = 2.0;
pub const SUB_DELAY_STEP: f64 = 0.5;
pub const SUB_DELAY_MIN: f64 = -10.0;
pub const SUB_DELAY_MAX: f64 = 10.0;

/// Which submenu the flyout is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Root,
    VideoSize,
    Speed,
    Subtitles,
    Audio,
}

/// Read-only inputs for one popup frame.
pub struct View<'a> {
    pub page: Page,
    pub tracks: &'a [Track],
    pub prefs: TorrentPlaybackPrefs,
    pub sub_scale: f64,
    pub sub_delay: f64,
}

/// Choices made this frame.
#[derive(Default)]
pub struct Out {
    pub page: Option<Page>,
    pub scale: Option<VideoScale>,
    pub speed: Option<f64>,
    pub audio: Option<i64>,
    /// `Some(None)` turns subtitles off.
    pub sub: Option<Option<i64>>,
    pub sub_scale: Option<f64>,
    pub sub_delay: Option<f64>,
}

pub fn paint(ui: &mut Ui, theme: &Theme, view: &View<'_>) -> Out {
    let mut out = Out::default();

    match view.page {
        Page::Root => root_page(ui, theme, view, &mut out),
        Page::VideoSize => video_size_page(ui, theme, view, &mut out),
        Page::Speed => speed_page(ui, theme, view, &mut out),
        Page::Subtitles => subtitles_page(ui, theme, view, &mut out),
        Page::Audio => audio_page(ui, theme, view, &mut out),
    }

    out
}

fn root_page(ui: &mut Ui, theme: &Theme, view: &View<'_>, out: &mut Out) {
    if submenu_row(ui, theme, Msg::VideoSize.t(), scale_label(view.prefs.scale)) {
        out.page = Some(Page::VideoSize);
    }

    if submenu_row(ui, theme, Msg::PlaybackSpeed.t(), &speed_label(view.prefs.speed)) {
        out.page = Some(Page::Speed);
    }

    if submenu_row(ui, theme, Msg::SubtitleTrack.t(), &selected_sub_label(view.tracks)) {
        out.page = Some(Page::Subtitles);
    }

    let audio_count = count_tracks(view.tracks, TrackKind::Audio);
    if audio_count <= 1 {
        return;
    }

    if submenu_row(ui, theme, Msg::AudioTrack.t(), &selected_audio_label(view.tracks)) {
        out.page = Some(Page::Audio);
    }
}

fn video_size_page(ui: &mut Ui, theme: &Theme, view: &View<'_>, out: &mut Out) {
    if back_row(ui, theme, Msg::VideoSize.t()) {
        out.page = Some(Page::Root);
        return;
    }

    for scale in VideoScale::ALL {
        let selected = *scale == view.prefs.scale;
        if radio_row(ui, theme, scale_label(*scale), selected) {
            out.scale = Some(*scale);
            out.page = Some(Page::Root);
        }
    }
}

fn speed_page(ui: &mut Ui, theme: &Theme, view: &View<'_>, out: &mut Out) {
    if back_row(ui, theme, Msg::PlaybackSpeed.t()) {
        out.page = Some(Page::Root);
        return;
    }

    for speed in SPEEDS {
        let selected = (view.prefs.speed - speed).abs() < 0.01;
        if radio_row(ui, theme, &speed_label(*speed), selected) {
            out.speed = Some(*speed);
            out.page = Some(Page::Root);
        }
    }
}

fn subtitles_page(ui: &mut Ui, theme: &Theme, view: &View<'_>, out: &mut Out) {
    if back_row(ui, theme, Msg::SubtitleTrack.t()) {
        out.page = Some(Page::Root);
        return;
    }

    let subs: Vec<&Track> = view
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Subtitle)
        .collect();
    let none_selected = !subs.iter().any(|track| track.selected);

    if radio_row(ui, theme, Msg::SubtitlesOff.t(), none_selected) {
        out.sub = Some(None);
    }

    for track in &subs {
        if radio_row(ui, theme, &track_label(track), track.selected) {
            out.sub = Some(Some(track.id));
        }
    }

    ui.add_space(6.0);
    ui.separator();

    let scale_text = format!("{:.0}%", view.sub_scale * 100.0);
    let scale_step = stepper_row(ui, theme, Msg::SubtitleSize.t(), &scale_text);
    if scale_step != 0.0 {
        let next = view.sub_scale + f64::from(scale_step) * SUB_SCALE_STEP;
        out.sub_scale = Some(next.clamp(SUB_SCALE_MIN, SUB_SCALE_MAX));
    }

    let delay_text = format!("{:+.1}s", view.sub_delay);
    let delay_step = stepper_row(ui, theme, Msg::SubtitleDelay.t(), &delay_text);
    if delay_step != 0.0 {
        let next = view.sub_delay + f64::from(delay_step) * SUB_DELAY_STEP;
        out.sub_delay = Some(next.clamp(SUB_DELAY_MIN, SUB_DELAY_MAX));
    }
}

fn audio_page(ui: &mut Ui, theme: &Theme, view: &View<'_>, out: &mut Out) {
    if back_row(ui, theme, Msg::AudioTrack.t()) {
        out.page = Some(Page::Root);
        return;
    }

    let audio = view
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio);

    for track in audio {
        if radio_row(ui, theme, &track_label(track), track.selected) {
            out.audio = Some(track.id);
            out.page = Some(Page::Root);
        }
    }
}

/// Human label for one mpv track.
pub fn track_label(track: &Track) -> String {
    let title = track.title.as_deref().filter(|s| !s.is_empty());
    let lang = track.lang.as_deref().filter(|s| !s.is_empty());

    match (title, lang) {
        (Some(title), Some(lang)) => format!("{title} ({lang})"),
        (Some(title), None) => title.to_owned(),
        (None, Some(lang)) => lang.to_owned(),
        (None, None) => format!("{} {}", Msg::TrackLabel.t(), track.id),
    }
}

pub fn scale_label(scale: VideoScale) -> &'static str {
    match scale {
        VideoScale::Default => Msg::ScaleDefault.t(),
        VideoScale::Expand => Msg::ScaleExpand.t(),
        VideoScale::Fill => Msg::ScaleFill.t(),
        VideoScale::Zoom115 => Msg::ScaleZoom115.t(),
        VideoScale::Zoom130 => Msg::ScaleZoom130.t(),
    }
}

pub fn speed_label(speed: f64) -> String {
    if (speed - 1.0).abs() < 0.01 {
        return Msg::SpeedNormal.t().to_owned();
    }

    format!("{speed}×")
}

fn count_tracks(tracks: &[Track], kind: TrackKind) -> usize {
    tracks.iter().filter(|track| track.kind == kind).count()
}

fn selected_audio_label(tracks: &[Track]) -> String {
    let selected = tracks
        .iter()
        .find(|track| track.kind == TrackKind::Audio && track.selected);

    selected.map(track_label).unwrap_or_default()
}

fn selected_sub_label(tracks: &[Track]) -> String {
    let selected = tracks
        .iter()
        .find(|track| track.kind == TrackKind::Subtitle && track.selected);

    selected
        .map(track_label)
        .unwrap_or_else(|| Msg::SubtitlesOff.t().to_owned())
}

fn hover_row(ui: &mut Ui, id_salt: &str) -> (egui::Rect, egui::Response) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let response = ui
        .interact(rect, ui.id().with(id_salt), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);

    (rect, response)
}

fn submenu_row(ui: &mut Ui, theme: &Theme, label: &str, value: &str) -> bool {
    let (rect, response) = hover_row(ui, label);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme.rounding(theme.radius_card), theme.widget_hover);
    }

    let inner = rect.shrink2(vec2(10.0, 4.0));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );

    row.label(
        RichText::new(label)
            .size(theme.text_body)
            .color(theme.label),
    );

    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(
            ICON_CHEVRON_RIGHT
                .rich_text()
                .size(theme.text_icon)
                .color(theme.muted),
        );
        ui.label(
            RichText::new(value)
                .size(theme.text_small)
                .color(theme.muted),
        );
    });

    response.clicked()
}

fn back_row(ui: &mut Ui, theme: &Theme, title: &str) -> bool {
    let (rect, response) = hover_row(ui, "back");
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme.rounding(theme.radius_card), theme.widget_hover);
    }

    let inner = rect.shrink2(vec2(10.0, 4.0));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );

    row.label(
        ICON_ARROW_BACK
            .rich_text()
            .size(theme.text_icon)
            .color(theme.title),
    );
    row.add_space(8.0);
    row.label(
        RichText::new(title)
            .size(theme.text_body)
            .color(theme.title),
    );

    ui.add_space(4.0);
    response.clicked()
}

fn radio_row(ui: &mut Ui, theme: &Theme, label: &str, selected: bool) -> bool {
    let (rect, response) = hover_row(ui, label);
    let idle = if selected {
        theme.card_selected
    } else {
        egui::Color32::TRANSPARENT
    };
    let fill = if response.hovered() {
        theme.widget_hover
    } else {
        idle
    };

    if fill != egui::Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect, theme.rounding(theme.radius_card), fill);
    }

    let inner = rect.shrink2(vec2(10.0, 4.0));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );

    row.label(
        RichText::new(label)
            .size(theme.text_body)
            .color(theme.label),
    );

    if selected {
        row.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                ICON_CHECK
                    .rich_text()
                    .size(theme.text_icon)
                    .color(theme.title),
            );
        });
    }

    response.clicked()
}

/// Label + value with −/+ buttons. Returns -1.0, 0.0, or 1.0.
fn stepper_row(ui: &mut Ui, theme: &Theme, label: &str, value: &str) -> f32 {
    let mut step = 0.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let inner = rect.shrink2(vec2(10.0, 4.0));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );

    row.label(
        RichText::new(label)
            .size(theme.text_small)
            .color(theme.label),
    );

    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let opts =
            crate::widgets::button::Opts::secondary(vec2(STEPPER_BTN, STEPPER_BTN)).pad_y(0.0);

        if crate::widgets::button::label(ui, theme, "+", opts) {
            step = 1.0;
        }

        ui.label(
            RichText::new(value)
                .size(theme.text_small)
                .color(theme.muted_bright),
        );

        if crate::widgets::button::label(ui, theme, "−", opts) {
            step = -1.0;
        }
    });

    step
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, kind: TrackKind, title: Option<&str>, lang: Option<&str>) -> Track {
        Track {
            id,
            kind,
            lang: lang.map(str::to_owned),
            title: title.map(str::to_owned),
            selected: false,
        }
    }

    #[test]
    fn track_label_prefers_title_then_lang() {
        let both = track(1, TrackKind::Audio, Some("Commentary"), Some("en"));
        assert_eq!(track_label(&both), "Commentary (en)");

        let lang_only = track(2, TrackKind::Subtitle, None, Some("ru"));
        assert_eq!(track_label(&lang_only), "ru");

        let bare = track(3, TrackKind::Subtitle, None, None);
        assert_eq!(track_label(&bare), "Track 3");
    }

    #[test]
    fn speed_label_marks_normal() {
        assert_eq!(speed_label(1.0), "Normal");
        assert_eq!(speed_label(1.5), "1.5×");
    }
}
