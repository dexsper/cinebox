//! In-player Settings flyout: nested menu for video size, speed, subtitles, audio.

use cinebox_core::{TorrentPlaybackPrefs, VideoScale};
use cinebox_player::{Features, Track, TrackKind};
use egui::{Align, FontId, Label, Layout, RichText, Sense, Ui, UiBuilder, pos2, vec2};
use egui_material_icons::icons::{ICON_ARROW_BACK, ICON_CHECK, ICON_CHEVRON_RIGHT};
use rust_i18n::t;

use crate::theme::Theme;
use crate::widgets::poster::{self, Wrap};
use crate::widgets::{button, focus};

const ROW_H: f32 = 36.0;
const ROW_GAP: f32 = 8.0;
const VALUE_MIN_W: f32 = 48.0;
const STEPPER_BTN: f32 = 26.0;

pub(super) const SPEEDS: &[f64] = &[0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

pub(super) const SUB_SCALE_STEP: f64 = 0.1;
pub(super) const SUB_SCALE_MIN: f64 = 0.5;
pub(super) const SUB_SCALE_MAX: f64 = 2.0;
pub(super) const SUB_DELAY_STEP: f64 = 0.5;
pub(super) const SUB_DELAY_MIN: f64 = -10.0;
pub(super) const SUB_DELAY_MAX: f64 = 10.0;

/// Which submenu the flyout is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Root,
    VideoSize,
    Speed,
    Subtitles,
    Audio,
}

/// Read-only inputs for one popup frame.
pub(super) struct View<'a> {
    pub page: Page,
    pub features: Features,
    pub tracks: &'a [Track],
    pub prefs: TorrentPlaybackPrefs,
    pub sub_scale: f64,
    pub sub_delay: f64,
    /// The submenu just left for Root, whose row takes the D-pad back.
    pub left: Option<Page>,
}

/// A subtitle track to show, or none at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SubtitlePick {
    Off,
    Track(i64),
}

impl SubtitlePick {
    /// The id the player selects; `None` turns subtitles off.
    #[must_use]
    pub(super) fn track_id(self) -> Option<i64> {
        match self {
            Self::Off => None,
            Self::Track(id) => Some(id),
        }
    }
}

/// Choices made this frame.
#[derive(Debug, Default)]
pub(super) struct Out {
    pub page: Option<Page>,
    pub scale: Option<VideoScale>,
    pub speed: Option<f64>,
    pub audio: Option<i64>,
    pub sub: Option<SubtitlePick>,
    pub sub_scale: Option<f64>,
    pub sub_delay: Option<f64>,
}

/// A row on the root page that opens a submenu.
struct Entry<'a> {
    page: Page,
    label: &'a str,
    /// The current choice on that page, shown on the right.
    value: &'a str,
}

pub(super) fn paint(ui: &mut Ui, theme: &Theme, view: &View<'_>) -> Out {
    match view.page {
        Page::Root => Out { page: root_page(ui, theme, view), ..Out::default() },
        Page::VideoSize => video_size_page(ui, theme, view),
        Page::Speed => speed_page(ui, theme, view),
        Page::Subtitles => subtitles_page(ui, theme, view),
        Page::Audio => audio_page(ui, theme, view),
    }
}

/// The submenu opened this frame.
fn root_page(ui: &mut Ui, theme: &Theme, view: &View<'_>) -> Option<Page> {
    let video_size = t!("player.video_size");
    let scale = scale_label(view.prefs.scale);
    let mut opened = submenu(ui, theme, view, &Entry { page: Page::VideoSize, label: &video_size, value: &scale });

    if view.features.speed {
        let playback_speed = t!("player.playback_speed");
        let speed = speed_label(view.prefs.speed);
        let entry = Entry { page: Page::Speed, label: &playback_speed, value: &speed };
        opened = submenu(ui, theme, view, &entry).or(opened);
    }

    let subtitle_track = t!("player.subtitle_track");
    let sub = selected_sub_label(view.tracks);
    opened = submenu(ui, theme, view, &Entry { page: Page::Subtitles, label: &subtitle_track, value: &sub }).or(opened);

    let audio_count = count_tracks(view.tracks, TrackKind::Audio);
    if audio_count <= 1 {
        return opened;
    }

    let audio_track = t!("player.audio_track");
    let audio = selected_audio_label(view.tracks);

    submenu(ui, theme, view, &Entry { page: Page::Audio, label: &audio_track, value: &audio }).or(opened)
}

/// `entry.page` when its row was clicked.
fn submenu(ui: &mut Ui, theme: &Theme, view: &View<'_>, entry: &Entry<'_>) -> Option<Page> {
    let row = submenu_row(ui, theme, entry.label, entry.value);
    if view.left == Some(entry.page) {
        focus::prefer(&row);
    }

    row.clicked().then_some(entry.page)
}

fn video_size_page(ui: &mut Ui, theme: &Theme, view: &View<'_>) -> Out {
    let mut out = Out::default();

    if back_row(ui, theme, t!("player.video_size").as_ref()) {
        out.page = Some(Page::Root);
        return out;
    }

    for scale in VideoScale::ALL {
        let is_selected = *scale == view.prefs.scale;
        if radio_row(ui, theme, scale_label(*scale).as_ref(), is_selected) {
            out.scale = Some(*scale);
            out.page = Some(Page::Root);
        }
    }

    out
}

fn speed_page(ui: &mut Ui, theme: &Theme, view: &View<'_>) -> Out {
    let mut out = Out::default();

    if back_row(ui, theme, t!("player.playback_speed").as_ref()) {
        out.page = Some(Page::Root);
        return out;
    }

    for speed in SPEEDS {
        let is_selected = (view.prefs.speed - speed).abs() < 0.01;
        if radio_row(ui, theme, &speed_label(*speed), is_selected) {
            out.speed = Some(*speed);
            out.page = Some(Page::Root);
        }
    }

    out
}

fn subtitles_page(ui: &mut Ui, theme: &Theme, view: &View<'_>) -> Out {
    let mut out = Out::default();

    if back_row(ui, theme, t!("player.subtitle_track").as_ref()) {
        out.page = Some(Page::Root);
        return out;
    }

    let subs: Vec<&Track> = view.tracks.iter().filter(|track| track.kind == TrackKind::Subtitle).collect();
    let is_none_selected = !subs.iter().any(|track| track.selected);

    if radio_row(ui, theme, t!("player.subtitles_off").as_ref(), is_none_selected) {
        out.sub = Some(SubtitlePick::Off);
    }

    for track in &subs {
        if radio_row(ui, theme, &track_label(track), track.selected) {
            out.sub = Some(SubtitlePick::Track(track.id));
        }
    }

    ui.add_space(6.0);
    ui.separator();

    let scale_text = format!("{:.0}%", view.sub_scale * 100.0);
    let scale_step = stepper_row(ui, theme, t!("player.subtitle_size").as_ref(), &scale_text);
    if scale_step != 0.0 {
        let next = view.sub_scale + f64::from(scale_step) * SUB_SCALE_STEP;
        out.sub_scale = Some(next.clamp(SUB_SCALE_MIN, SUB_SCALE_MAX));
    }

    if !view.features.subtitle_delay {
        return out;
    }

    let delay_text = format!("{:+.1}s", view.sub_delay);
    let delay_step = stepper_row(ui, theme, t!("player.subtitle_delay").as_ref(), &delay_text);
    if delay_step != 0.0 {
        let next = view.sub_delay + f64::from(delay_step) * SUB_DELAY_STEP;
        out.sub_delay = Some(next.clamp(SUB_DELAY_MIN, SUB_DELAY_MAX));
    }

    out
}

fn audio_page(ui: &mut Ui, theme: &Theme, view: &View<'_>) -> Out {
    let mut out = Out::default();

    if back_row(ui, theme, t!("player.audio_track").as_ref()) {
        out.page = Some(Page::Root);
        return out;
    }

    let audio = view.tracks.iter().filter(|track| track.kind == TrackKind::Audio);

    for track in audio {
        if radio_row(ui, theme, &track_label(track), track.selected) {
            out.audio = Some(track.id);
            out.page = Some(Page::Root);
        }
    }

    out
}

/// Human label for one mpv track.
pub(super) fn track_label(track: &Track) -> String {
    let title = track.title.as_deref().filter(|title| !title.is_empty());
    let lang = track.lang.as_deref().filter(|lang| !lang.is_empty());

    match (title, lang) {
        (Some(title), Some(lang)) => format!("{title} ({lang})"),
        (Some(title), None) => title.to_owned(),
        (None, Some(lang)) => lang.to_owned(),
        (None, None) => format!("{} {}", t!("player.track"), track.id),
    }
}

pub(super) fn scale_label(scale: VideoScale) -> std::borrow::Cow<'static, str> {
    match scale {
        VideoScale::Default => t!("player.scale.default"),
        VideoScale::Expand => t!("player.scale.expand"),
        VideoScale::Fill => t!("player.scale.fill"),
        VideoScale::Zoom115 => t!("player.scale.zoom115"),
        VideoScale::Zoom130 => t!("player.scale.zoom130"),
    }
}

pub(super) fn speed_label(speed: f64) -> String {
    if (speed - 1.0).abs() < 0.01 {
        return t!("player.speed_normal").into_owned();
    }

    format!("{speed}×")
}

fn count_tracks(tracks: &[Track], kind: TrackKind) -> usize {
    tracks.iter().filter(|track| track.kind == kind).count()
}

fn selected_audio_label(tracks: &[Track]) -> String {
    let selected = tracks.iter().find(|track| track.kind == TrackKind::Audio && track.selected);

    selected.map(track_label).unwrap_or_default()
}

fn selected_sub_label(tracks: &[Track]) -> String {
    let selected = tracks.iter().find(|track| track.kind == TrackKind::Subtitle && track.selected);

    selected.map_or_else(|| t!("player.subtitles_off").into_owned(), track_label)
}

fn hover_row(ui: &mut Ui, id_salt: &str) -> (egui::Rect, egui::Response) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let response = button::pointing(ui.interact(rect, ui.id().with(id_salt), Sense::click()));

    (rect, response)
}

/// How much of `inner_w` the left label may take vs the right-hand value.
///
/// The value shrinks first; the label only yields once the value is down to
/// [`VALUE_MIN_W`]. Trailing is the chevron/check width (gap added here).
fn split_row_widths(inner_w: f32, label_desired: f32, trailing_w: f32, has_value: bool) -> (f32, f32) {
    let after_icon = (inner_w - trailing_w).max(0.0);

    if !has_value {
        return (label_desired.min(after_icon), 0.0);
    }

    let usable = (after_icon - 2.0 * ROW_GAP).max(0.0);
    let max_label = (usable - VALUE_MIN_W).max(0.0);
    let label = label_desired.min(max_label);
    let value = (usable - label).max(0.0);

    (label, value)
}

fn galley_y(rect: egui::Rect, height: f32) -> f32 {
    rect.center().y - height * 0.5
}

fn submenu_row(ui: &mut Ui, theme: &Theme, label: &str, value: &str) -> egui::Response {
    let (rect, mut response) = hover_row(ui, label);
    if focus::lit(&response) {
        ui.painter().rect_filled(rect, theme.rounding(theme.radius_card), theme.widget_hover);
    }

    let inner = rect.shrink2(vec2(10.0, 4.0));
    let has_value = !value.is_empty();

    let icon = ICON_CHEVRON_RIGHT;
    let icon_font = FontId::new(theme.text_icon, icon.font_family());
    let icon_text = icon.codepoint.to_owned();
    let icon_galley = ui.painter().layout_no_wrap(icon_text, icon_font, theme.muted);
    let icon_w = icon_galley.size().x;

    let label_font = theme.ui_font(theme.text_body);
    let label_text = label.to_owned();
    let label_full = ui.painter().layout_no_wrap(label_text, label_font, theme.label);
    let label_desired = label_full.size().x;
    let inner_w = inner.width();
    let (label_budget, value_budget) = split_row_widths(inner_w, label_desired, icon_w, has_value);

    let label_width = label_budget.max(1.0);
    let label_wrap = Wrap { color: theme.label, font_size: theme.text_body, width: label_width, max_rows: 1 };
    let label_galley = poster::wrap_lines(ui, theme, label, label_wrap);

    let label_pos = pos2(inner.left(), galley_y(inner, label_galley.size().y));
    ui.painter().galley(label_pos, label_galley, theme.label);

    let icon_pos = pos2(inner.right() - icon_w, galley_y(inner, icon_galley.size().y));
    ui.painter().galley(icon_pos, icon_galley, theme.muted);

    if !has_value {
        return response;
    }

    let value_font = theme.ui_font(theme.text_small);
    let value_text = value.to_owned();
    let value_full = ui.painter().layout_no_wrap(value_text, value_font, theme.muted);
    let value_desired = value_full.size().x;

    if value_desired > value_budget + 0.5 {
        response = response.on_hover_text(value);
    }

    let value_width = value_budget.max(1.0);
    let value_wrap = Wrap { color: theme.muted, font_size: theme.text_small, width: value_width, max_rows: 1 };
    let value_galley = poster::wrap_lines(ui, theme, value, value_wrap);

    let value_x = icon_pos.x - ROW_GAP - value_galley.size().x;
    let value_pos = pos2(value_x, galley_y(inner, value_galley.size().y));
    ui.painter().galley(value_pos, value_galley, theme.muted);

    response
}

fn back_row(ui: &mut Ui, theme: &Theme, title: &str) -> bool {
    let (rect, response) = hover_row(ui, "back");
    if focus::lit(&response) {
        ui.painter().rect_filled(rect, theme.rounding(theme.radius_card), theme.widget_hover);
    }

    let inner = rect.shrink2(vec2(10.0, 4.0));
    let mut row = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));

    row.label(ICON_ARROW_BACK.rich_text().size(theme.text_icon).color(theme.title));
    row.add_space(8.0);
    row.label(RichText::new(title).size(theme.text_body).color(theme.title));

    ui.add_space(4.0);
    response.clicked()
}

fn radio_row(ui: &mut Ui, theme: &Theme, label: &str, is_selected: bool) -> bool {
    let (rect, mut response) = hover_row(ui, label);
    if is_selected {
        focus::prefer(&response);
    }

    let idle = if is_selected { theme.card_selected } else { egui::Color32::TRANSPARENT };
    let fill = if focus::lit(&response) { theme.widget_hover } else { idle };

    if fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, theme.rounding(theme.radius_card), fill);
    }

    let inner = rect.shrink2(vec2(10.0, 4.0));

    let check = ICON_CHECK;
    let check_font = FontId::new(theme.text_icon, check.font_family());
    let check_text = check.codepoint.to_owned();
    let check_galley = ui.painter().layout_no_wrap(check_text, check_font, theme.title);

    let trailing = if is_selected { check_galley.size().x + ROW_GAP } else { 0.0 };

    let text_w = (inner.width() - trailing).max(1.0);
    let label_font = theme.ui_font(theme.text_body);
    let label_text = label.to_owned();
    let label_full = ui.painter().layout_no_wrap(label_text, label_font, theme.label);
    let label_desired = label_full.size().x;

    if label_desired > text_w + 0.5 {
        response = response.on_hover_text(label);
    }

    let label_wrap = Wrap { color: theme.label, font_size: theme.text_body, width: text_w, max_rows: 1 };
    let label_galley = poster::wrap_lines(ui, theme, label, label_wrap);

    let label_pos = pos2(inner.left(), galley_y(inner, label_galley.size().y));
    ui.painter().galley(label_pos, label_galley, theme.label);

    if is_selected {
        let check_pos = pos2(inner.right() - check_galley.size().x, galley_y(inner, check_galley.size().y));
        ui.painter().galley(check_pos, check_galley, theme.title);
    }

    response.clicked()
}

/// Label and value with minus and plus buttons. -1.0, 0.0 or 1.0 for the button pressed.
fn stepper_row(ui: &mut Ui, theme: &Theme, label: &str, value: &str) -> f32 {
    let mut step = 0.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let inner = rect.shrink2(vec2(10.0, 4.0));

    let value_font = theme.ui_font(theme.text_small);
    let value_text = value.to_owned();
    let value_galley = ui.painter().layout_no_wrap(value_text, value_font, theme.muted_bright);
    let value_w = value_galley.size().x;

    let cluster = STEPPER_BTN * 2.0 + 6.0 * 2.0 + value_w;
    let label_w = (inner.width() - cluster - ROW_GAP).max(1.0);

    let mut row = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));

    row.add_sized(
        vec2(label_w, inner.height()),
        Label::new(RichText::new(label).size(theme.text_small).color(theme.label)).truncate(),
    );

    row.with_layout(Layout::right_to_left(Align::Center), |buttons| {
        buttons.spacing_mut().item_spacing.x = 6.0;
        let opts = button::Opts::secondary(vec2(STEPPER_BTN, STEPPER_BTN)).pad_y(0.0);

        if button::label(buttons, theme, "+", opts) {
            step = 1.0;
        }

        buttons.label(RichText::new(value).size(theme.text_small).color(theme.muted_bright));

        if button::label(buttons, theme, "−", opts) {
            step = -1.0;
        }
    });

    step
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, kind: TrackKind, title: Option<&str>, lang: Option<&str>) -> Track {
        Track { id, kind, lang: lang.map(str::to_owned), title: title.map(str::to_owned), selected: false }
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

    #[test]
    fn split_row_widths_keeps_short_label_and_gives_rest_to_value() {
        let (label, value) = split_row_widths(200.0, 60.0, 16.0, true);

        assert!((label - 60.0).abs() < f32::EPSILON);
        assert!((value - 108.0).abs() < f32::EPSILON);
    }

    #[test]
    fn split_row_widths_shrinks_value_before_label() {
        let (label, value) = split_row_widths(120.0, 200.0, 16.0, true);

        assert!((label - 40.0).abs() < f32::EPSILON);
        assert!((value - VALUE_MIN_W).abs() < f32::EPSILON);
    }

    #[test]
    fn split_row_widths_without_value_gives_all_to_label() {
        let (label, value) = split_row_widths(100.0, 80.0, 16.0, false);

        assert!((label - 80.0).abs() < f32::EPSILON);
        assert_eq!(value, 0.0);
    }
}
