//! Hit list, sort, and filters.

use cinebox_core::{MediaKind, QualityBand};
use cinebox_indexer::{
    AudioLang, SortMode, TriChoice, VoiceFilter, hit_bitrate_mbps, season_options,
    voice_filter_options, year_options,
};
use egui::{
    Align, Color32, CornerRadius, Frame, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui,
    Vec2, pos2, vec2,
};
use egui_material_icons::icons::{ICON_FILTER_LIST, ICON_RESTART_ALT};
use rust_i18n::t;

use super::state::{TorrentHits, TorrentState};
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::drawer::Overlay;
use crate::widgets::page_state::{ErrorChoice, error_page};
use crate::widgets::{self, chips, combo, focus, multiselect, scroll};

const FILTERS_BTN_W: f32 = 152.0;

pub(super) fn list_pane(
    ui: &mut Ui,
    state: &mut TorrentState,
    theme: &Theme,
    error_choice: &mut Option<ErrorChoice>,
    pick: &mut Option<usize>,
    t: f32,
    filters: &mut Overlay,
) {
    if t < 0.22 {
        return;
    }

    toolbar(ui, state, theme, filters);
    state.apply_filter_sort();

    match &state.hits {
        TorrentHits::Loading => {
            widgets::page_spinner(ui, theme);
        }
        TorrentHits::Failed(error) => {
            *error_choice = error_page(ui, theme, error);
        }
        TorrentHits::Ready(hits) => {
            let visible = &state.visible;
            let rows = &mut state.rows;

            ui.label(
                RichText::new(format!("{} / {}", visible.len(), hits.len()))
                    .size(theme.text_small)
                    .color(theme.label),
            );

            if visible.is_empty() {
                widgets::page_message(ui, theme, t!("torrents.none").as_ref(), theme.muted);
                return;
            }

            scroll::vertical(ui, "torrent-hits", |ui| {
                let ring_room = egui::Margin {
                    left: RING_INSET,
                    right: 0,
                    top: RING_INSET,
                    bottom: 0,
                };

                let context = RowContext {
                    kind: state.kind,
                    runtime: state.runtime_minutes,
                    year: state.year,
                };

                Frame::new().inner_margin(ring_room).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    let focused = ui.memory(|mem| mem.focused());
                    let keep = visible.iter().position(|&index| {
                        let hit = hits.get(index);
                        hit.is_some_and(|hit| Some(hit_id(ui, hit)) == focused)
                    });

                    rows.show(ui, visible.len(), keep, |ui, row| {
                        let index = visible[row];
                        let Some(hit) = hits.get(index) else {
                            return;
                        };

                        hit_row(ui, hit, context, theme, pick, index);
                    });
                });

                ui.add_space(LIST_BOTTOM_PAD);
            });
        }
    }
}

fn toolbar(ui: &mut Ui, state: &mut TorrentState, theme: &Theme, filters: &mut Overlay) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let combo_w = (ui.available_width() - FILTERS_BTN_W - 8.0).max(160.0);

        ui.allocate_ui_with_layout(
            vec2(combo_w, combo::HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                combo::show_with(
                    ui,
                    theme,
                    "torrent-sort",
                    &mut state.sort,
                    SortMode::ALL,
                    |mode| sort_label(mode).into_owned(),
                );
            },
        );

        let open = filters.is_open();
        if filters_button(ui, theme, open, state.filter.active_count()) {
            filters.toggle(ui.input(|i| i.time));
        }
    });
}

fn filters_button(ui: &mut Ui, theme: &Theme, open: bool, count: usize) -> bool {
    let active = count > 0;
    let label = filters_label(active);
    let selected = open || active;
    let size = vec2(FILTERS_BTN_W, combo::HEIGHT);
    let pad_y = button::icon_label_pad_y(ui, theme, ICON_FILTER_LIST, combo::HEIGHT);
    let opts = Opts::secondary(size).selected(selected).pad_y(pad_y);
    let fg = theme.title;
    let icon = ICON_FILTER_LIST.rich_text().size(theme.text_icon).color(fg);
    let text = RichText::new(&label).size(theme.text_body).color(fg);
    let atoms = (egui::Atom::grow(), icon, text, egui::Atom::grow());

    let response = button::add_named(ui, theme, atoms, opts, Some(&label));
    if count > 0 {
        paint_count_badge(ui, response.rect, count, theme);
    }

    response.clicked()
}

fn paint_count_badge(ui: &Ui, button: Rect, count: usize, theme: &Theme) {
    let text = count.to_string();
    let font = theme.ui_font(theme.text_caption);
    let galley = ui.painter().layout_no_wrap(text, font, Color32::WHITE);
    let radius = 9.0;
    let center = pos2(button.right() - 2.0, button.top() + 2.0);
    ui.painter().circle_filled(center, radius, theme.err);

    let x = center.x - galley.size().x * 0.5;
    let y = center.y - galley.size().y * 0.5;
    ui.painter().galley(pos2(x, y), galley, Color32::WHITE);
}

fn filters_label(active: bool) -> String {
    if active {
        return format!("{} · {}", t!("filter.filters"), t!("filter.on"));
    }

    t!("filter.filters").into_owned()
}

pub(super) fn filters_drawer(ui: &mut Ui, state: &mut TorrentState, theme: &Theme) {
    ui.label(
        RichText::new(t!("filter.filters").as_ref())
            .font(theme.title_font(theme.text_display))
            .color(theme.title),
    );

    ui.add_space(12.0);
    scroll::vertical(ui, "torrent-filters", |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        section_label(ui, theme, t!("filter.quality").as_ref());
        chips::multi_row(
            ui,
            theme,
            &mut state.filter.quality,
            QualityBand::ALL,
            |band| band.label().to_owned(),
        );

        section_label(ui, theme, t!("filter.hdr").as_ref());
        tri_row(ui, theme, &mut state.filter.hdr);
        section_label(ui, theme, t!("filter.dolby").as_ref());
        tri_row(ui, theme, &mut state.filter.dolby);
        section_label(ui, theme, t!("filter.subs").as_ref());
        tri_row(ui, theme, &mut state.filter.subs);

        let hits = match &state.hits {
            TorrentHits::Ready(hits) => hits.as_slice(),
            _ => &[],
        };

        let voices = voice_filter_options(hits, &state.filter.voice);
        section_label(ui, theme, t!("filter.translation").as_ref());
        multiselect::show_with(
            ui,
            theme,
            "torrent-voice",
            &mut state.filter.voice,
            &voices,
            |voice| voice_label(voice).into_owned(),
        );

        section_label(ui, theme, t!("filter.language").as_ref());
        multiselect::show_with(
            ui,
            theme,
            "torrent-lang",
            &mut state.filter.lang,
            AudioLang::ALL,
            |lang| audio_lang_label(lang).into_owned(),
        );

        let years = year_options(hits, state.year, &state.filter.year);
        if years.len() > 1 {
            section_label(ui, theme, t!("filter.year").as_ref());
            multiselect::show_with(
                ui,
                theme,
                "torrent-year",
                &mut state.filter.year,
                &years,
                |year| year.to_string(),
            );
        }

        if state.kind == MediaKind::Tv {
            let seasons = season_options(hits);
            if !seasons.is_empty() {
                section_label(ui, theme, t!("media.season").as_ref());
                multiselect::show_with(
                    ui,
                    theme,
                    "torrent-season",
                    &mut state.filter.season,
                    &seasons,
                    |season| format!("S{season}"),
                );
            }
        }

        ui.add_space(8.0);
        if reset_button(ui, theme) {
            state.filter = cinebox_indexer::TorrentFilter::default();
        }
    });

    state.apply_filter_sort();
}

fn section_label(ui: &mut Ui, theme: &Theme, label: &str) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(label)
            .size(theme.text_small)
            .color(theme.muted_bright),
    );
}

fn tri_row(ui: &mut Ui, theme: &Theme, value: &mut TriChoice) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for choice in TriChoice::ALL {
            if chip(ui, theme, tri_label(*choice).as_ref(), *value == *choice) {
                *value = *choice;
            }
        }
    });
}

fn tri_label(choice: TriChoice) -> std::borrow::Cow<'static, str> {
    match choice {
        TriChoice::Any => t!("filter.any"),
        TriChoice::Yes => t!("common.yes"),
        TriChoice::No => t!("common.no"),
    }
}

fn sort_label(mode: SortMode) -> std::borrow::Cow<'static, str> {
    match mode {
        SortMode::Popular => t!("filter.sort_popular"),
        SortMode::Seeders => t!("filter.sort_seeders"),
        SortMode::Size => t!("filter.sort_size"),
    }
}

fn voice_label(filter: VoiceFilter) -> std::borrow::Cow<'static, str> {
    match filter {
        VoiceFilter::Dubbing => t!("filter.voice.dubbing"),
        VoiceFilter::Polyphonic => t!("filter.voice.polyphonic"),
        VoiceFilter::TwoVoice => t!("filter.voice.two_voice"),
        VoiceFilter::Amateur => t!("filter.voice.amateur"),
        VoiceFilter::Studio(name) => std::borrow::Cow::Borrowed(name),
    }
}

fn audio_lang_label(lang: AudioLang) -> std::borrow::Cow<'static, str> {
    match lang {
        AudioLang::Ru => t!("lang.ru"),
        AudioLang::En => t!("lang.en"),
        AudioLang::Uk => t!("lang.uk"),
        AudioLang::Ja => t!("lang.ja"),
        AudioLang::Ko => t!("lang.ko"),
        AudioLang::Zh => t!("lang.zh"),
        AudioLang::De => t!("lang.de"),
        AudioLang::Fr => t!("lang.fr"),
    }
}

fn chip(ui: &mut Ui, theme: &Theme, label: &str, active: bool) -> bool {
    button::label(ui, theme, label, Opts::chip(active))
}

fn reset_button(ui: &mut Ui, theme: &Theme) -> bool {
    let width = ui.available_width();
    button::icon_label(
        ui,
        theme,
        ICON_RESTART_ALT,
        t!("filter.reset").as_ref(),
        Opts::secondary(vec2(width, 36.0)),
    )
}

/// What every row needs from the title the list is for.
#[derive(Clone, Copy)]
struct RowContext {
    kind: MediaKind,
    runtime: Option<u32>,
    year: Option<u16>,
}

/// The names, then a line of tags read from the title, then where and how
/// well it is shared.
fn hit_row(
    ui: &mut Ui,
    hit: &cinebox_indexer::TorrentHit,
    context: RowContext,
    theme: &Theme,
    pick: &mut Option<usize>,
    index: usize,
) {
    let id = hit_id(ui, hit);
    let shown = Frame::new()
        .fill(theme.card)
        .corner_radius(theme.rounding(theme.radius_card))
        .inner_margin(egui::Margin::symmetric(16, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let name = name_galley(ui, &hit.name, theme);
            ui.add(egui::Label::new(name).selectable(false));

            let tags = row_tags(hit, context);
            if !tags.is_empty() {
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    for tag in &tags {
                        paint_tag(ui, theme, tag);
                    }
                });
            }

            ui.add_space(12.0);
            meta_line(ui, hit, context, theme);
        });

    let response = button::click_rect(ui, id, shown.response.rect);
    if focus::own_mark(&response) {
        hit_ring(ui, shown.response.rect, theme);
    }

    let response = response.on_hover_text(&hit.display_title);
    if response.clicked() && !hit.magnet.is_empty() {
        *pick = Some(index);
    }
}

/// Up to two lines; the full title has no place left that needs it.
fn name_galley(ui: &Ui, name: &str, theme: &Theme) -> std::sync::Arc<egui::Galley> {
    let font = theme.title_font(theme.text_subtitle);
    let width = ui.available_width();
    let mut job = egui::text::LayoutJob::simple(name.to_owned(), font, theme.title, width);
    job.wrap.max_rows = 2;
    job.wrap.overflow_character = Some('…');

    ui.painter().layout_job(job)
}

/// Date, tracker and the started mark on the left; sharing and size on the right.
fn meta_line(ui: &mut Ui, hit: &cinebox_indexer::TorrentHit, context: RowContext, theme: &Theme) {
    let bitrate = format_bitrate(context.kind, hit_bitrate_mbps(hit, context.runtime));

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let size_label = hit.size_label();
            pill(ui, theme, &size_label, theme.size_pill_bg, theme.size_pill_fg);
            metrics_bar(
                ui,
                theme,
                bitrate.as_deref(),
                &hit.seeders.to_string(),
                &hit.peers.to_string(),
            );

            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let date = crate::i18n::format_release_date(&hit.published);
                let date = if date.is_empty() { String::from("—") } else { date };

                ui.label(RichText::new(date).size(META_SIZE).color(theme.muted));

                if hit.local_rank.is_some() || hit.started {
                    let started = RichText::new(t!("torrents.started").as_ref())
                        .size(META_SIZE)
                        .color(theme.ok);
                    ui.label(started);
                }

                // Aggregators list every tracker that has it; that list gives way first.
                let trackers = RichText::new(&hit.tracker).size(META_SIZE).color(theme.muted);
                ui.add(egui::Label::new(trackers).truncate());
            });
        });
    });
}

/// How loud a tag is: what decides the picture first, then the sound, then who voiced it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TagTone {
    Picture,
    Sound,
    Studio,
}

struct Tag {
    text: String,
    tone: TagTone,
}

impl Tag {
    fn new(text: impl Into<String>, tone: TagTone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

/// Studios after these many fold into a `+N`.
const STUDIOS_SHOWN: usize = 3;

fn row_tags(hit: &cinebox_indexer::TorrentHit, context: RowContext) -> Vec<Tag> {
    let info = &hit.info;
    let mut tags = Vec::new();

    if context.kind == MediaKind::Tv {
        if let Some(text) = season_tag(info) {
            tags.push(Tag::new(text, TagTone::Picture));
        }
    }

    let year = info.year.filter(|year| Some(*year) != context.year);
    if let Some(year) = year {
        tags.push(Tag::new(year.to_string(), TagTone::Picture));
    }

    let picture = [
        info.resolution.map(|band| band.as_str()),
        info.hdr.map(hdr_label),
        info.quality.map(|quality| quality.as_str()),
        info.codec.map(|codec| codec.as_str()),
    ];
    for text in picture.into_iter().flatten() {
        tags.push(Tag::new(text, TagTone::Picture));
    }

    if let Some(edition) = info.edition {
        tags.push(Tag::new(edition_label(edition), TagTone::Picture));
    }

    for (kind, count) in &hit.audio.tracks {
        let label = voiceover_label(*kind);
        let text = if *count > 1 { format!("{label} ×{count}") } else { label.into_owned() };
        tags.push(Tag::new(text, TagTone::Sound));
    }

    if hit.audio.original {
        tags.push(Tag::new(t!("torrents.original"), TagTone::Sound));
    }

    if hit.audio.subtitles {
        tags.push(Tag::new(t!("torrents.subtitles"), TagTone::Sound));
    }

    for studio in hit.voices.iter().take(STUDIOS_SHOWN) {
        tags.push(Tag::new(*studio, TagTone::Studio));
    }

    let more = hit.voices.len().saturating_sub(STUDIOS_SHOWN);
    if more > 0 {
        tags.push(Tag::new(format!("+{more}"), TagTone::Studio));
    }

    tags
}

/// `Season 2 · Episodes 1–8`, `Seasons 1–3`, or the episodes alone.
fn season_tag(info: &cinebox_indexer::TitleInfo) -> Option<String> {
    let seasons = info.seasons_named.then(|| {
        let word = if info.seasons.len() > 1 { t!("torrents.seasons") } else { t!("media.season") };
        format!("{word} {}", info.season_label().replace('-', "–"))
    });

    let episodes = info.episodes.map(|span| {
        if span.from == span.to {
            return format!("{} {}", t!("media.episode"), span.to);
        }

        format!("{} {}–{}", t!("torrents.episodes"), span.from, span.to)
    });

    match (seasons, episodes) {
        (Some(seasons), Some(episodes)) => Some(format!("{seasons}  ·  {episodes}")),
        (seasons, episodes) => seasons.or(episodes),
    }
}

fn hdr_label(hdr: cinebox_indexer::Hdr) -> &'static str {
    match hdr {
        cinebox_indexer::Hdr::Hdr => "HDR",
        cinebox_indexer::Hdr::DolbyVision => "Dolby Vision",
    }
}

fn edition_label(edition: cinebox_indexer::Edition) -> std::borrow::Cow<'static, str> {
    match edition {
        cinebox_indexer::Edition::Imax => t!("torrents.edition.imax"),
        cinebox_indexer::Edition::Extended => t!("torrents.edition.extended"),
        cinebox_indexer::Edition::DirectorsCut => t!("torrents.edition.directors"),
    }
}

fn voiceover_label(kind: cinebox_indexer::Voiceover) -> std::borrow::Cow<'static, str> {
    match kind {
        cinebox_indexer::Voiceover::Dubbing => t!("torrents.track.dubbing"),
        cinebox_indexer::Voiceover::Polyphonic => t!("torrents.track.polyphonic"),
        cinebox_indexer::Voiceover::TwoVoice => t!("torrents.track.two_voice"),
        cinebox_indexer::Voiceover::Author => t!("torrents.track.author"),
        cinebox_indexer::Voiceover::OneVoice => t!("torrents.track.one_voice"),
    }
}

/// One allocation per tag, not a `Frame`: a child `Ui` cannot move to the
/// next row, so a long line of tags would overflow the row instead of wrapping.
fn paint_tag(ui: &mut Ui, theme: &Theme, tag: &Tag) {
    let (fill, stroke, text) = match tag.tone {
        TagTone::Picture => (theme.metric_bg, Stroke::NONE, theme.title),
        TagTone::Sound => (theme.size_pill_bg, Stroke::NONE, theme.label),
        TagTone::Studio => (Color32::TRANSPARENT, Stroke::new(1.0, theme.window_edge), theme.muted_bright),
    };

    let font = theme.ui_font(TAG_SIZE);
    let galley = ui.painter().layout_no_wrap(tag.text.clone(), font, text);
    let size = vec2(galley.size().x + 2.0 * TAG_PAD_X, TAG_H);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

    ui.painter()
        .rect(rect, CornerRadius::same(4), fill, stroke, StrokeKind::Inside);
    let pos = pos2(rect.left() + TAG_PAD_X, rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(pos, galley, text);
}

/// Not by position: a release just watched moves up the list, and focus
/// coming back to it must follow.
fn hit_id(ui: &Ui, hit: &cinebox_indexer::TorrentHit) -> egui::Id {
    ui.id().with(("torrent-hit", &hit.tracker, &hit.title, hit.size_bytes))
}

/// Same ring as the poster hover, with a tighter gap to the row.
fn hit_ring(ui: &Ui, rect: Rect, theme: &Theme) {
    let pad = theme.ring_w + HIT_RING_GAP;
    let ring = rect.expand(pad);
    let radius = theme.radius_card + pad;

    ui.painter().rect_stroke(
        ring,
        CornerRadius::same(radius.round() as u8),
        Stroke::new(theme.ring_w, theme.ring),
        StrokeKind::Inside,
    );
}

const HIT_RING_GAP: f32 = 2.0;
/// Left/top margin inside the scroll viewport so the hover ring is not clipped.
const RING_INSET: i8 = 6;
const LIST_BOTTOM_PAD: f32 = 24.0;

const METRIC_VAL_H: f32 = 18.0;
const METRIC_GAP: f32 = 14.0;
const METRIC_LABEL_GAP: f32 = 6.0;
/// Date, tracker and the metric labels.
const META_SIZE: f32 = 14.0;
const TAG_SIZE: f32 = 13.0;
const TAG_H: f32 = 24.0;
const TAG_PAD_X: f32 = 8.0;

pub(super) fn format_bitrate(kind: MediaKind, mbps: Option<f64>) -> Option<String> {
    if kind != MediaKind::Movie {
        return None;
    }

    Some(
        mbps.map(|mbps| format!("{mbps:.1}"))
            .unwrap_or_else(|| String::from("—")),
    )
}

/// Pairs go straight into the right-to-left row (a nested `horizontal` breaks
/// vertical centering), so on screen this reads Bitrate, Seeds, Leechers.
fn metrics_bar(ui: &mut Ui, theme: &Theme, bitrate: Option<&str>, seeds: &str, leechers: &str) {
    metric_pair(ui, t!("torrents.leechers").as_ref(), leechers, theme);
    metric_pair(ui, t!("torrents.seeds").as_ref(), seeds, theme);

    let Some(bitrate) = bitrate else {
        return;
    };

    metric_pair(ui, t!("torrents.bitrate").as_ref(), bitrate, theme);
}

/// `item_spacing` applies after a widget, so each widget sets the gap that follows it.
fn metric_pair(ui: &mut Ui, label: &str, value: &str, theme: &Theme) {
    ui.spacing_mut().item_spacing.x = METRIC_LABEL_GAP;
    pill(ui, theme, value, theme.metric_bg, theme.title);

    ui.spacing_mut().item_spacing.x = METRIC_GAP;
    ui.label(RichText::new(label).size(META_SIZE).color(theme.muted));
}

/// Fixed `METRIC_VAL_H` content height keeps every pill in the row the same height.
fn pill(ui: &mut Ui, theme: &Theme, text: &str, bg: Color32, fg: Color32) {
    Frame::new()
        .fill(bg)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(9, 4))
        .show(ui, |ui| {
            let font = theme.ui_font(META_SIZE);
            let galley = ui.painter().layout_no_wrap(text.to_owned(), font, fg);
            let size = Vec2::new(galley.size().x, METRIC_VAL_H);
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let pos = pos2(rect.left(), rect.center().y - galley.size().y * 0.5);

            ui.painter().galley(pos, galley, fg);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_only_for_movies() {
        assert_eq!(
            format_bitrate(MediaKind::Movie, Some(8.2)).as_deref(),
            Some("8.2")
        );

        assert_eq!(format_bitrate(MediaKind::Movie, None).as_deref(), Some("—"));
        assert_eq!(format_bitrate(MediaKind::Tv, Some(8.2)), None);
        assert_eq!(format_bitrate(MediaKind::Tv, None), None);
    }
}
