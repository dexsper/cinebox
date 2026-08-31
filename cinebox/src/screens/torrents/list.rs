//! Hit list, sort, and filters.

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaKind, QualityBand, typograph};
use cinebox_parse::{
    AudioLang, SortMode, TriChoice, VoiceFilter, filtered_hits, hit_bitrate_mbps, season_options,
    voice_filter_options, year_options,
};
use egui::{
    Align, Color32, CornerRadius, Frame, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui,
    Vec2, pos2, vec2,
};
use egui_material_icons::icons::{ICON_FILTER_LIST, ICON_RESTART_ALT};

use super::state::{TorrentHits, TorrentState};
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::drawer::Overlay;
use crate::widgets::{self, chips, combo, multiselect, scroll};

const FILTERS_BTN_W: f32 = 152.0;

pub(super) fn list_pane(
    ui: &mut Ui,
    state: &mut TorrentState,
    theme: &Theme,
    retry: &mut bool,
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
            if widgets::page_error(ui, theme, error) {
                *retry = true;
            }
        }
        TorrentHits::Ready(hits) => {
            let visible: Vec<(usize, &cinebox_parse::TorrentHit)> =
                filtered_hits(hits, &state.filter).collect();

            ui.label(
                RichText::new(format!("{} / {}", visible.len(), hits.len()))
                    .size(theme.text_small)
                    .color(theme.label),
            );

            if visible.is_empty() {
                widgets::page_message(ui, theme, Msg::NoTorrents.en(), theme.muted);
                return;
            }

            scroll::vertical(ui, "torrent-hits", |ui| {
                let ring_room = egui::Margin {
                    left: RING_INSET,
                    right: 0,
                    top: RING_INSET,
                    bottom: 0,
                };

                Frame::new().inner_margin(ring_room).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for (index, hit) in visible {
                        hit_row(
                            ui,
                            hit,
                            state.kind,
                            state.runtime_minutes,
                            theme,
                            pick,
                            index,
                        );
                    }
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
                    |mode| sort_label(mode).to_owned(),
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
        return format!("{} · {}", Msg::Filters.en(), Msg::FilterOn.en());
    }

    Msg::Filters.en().to_owned()
}

pub(super) fn filters_drawer(ui: &mut Ui, state: &mut TorrentState, theme: &Theme) {
    ui.label(
        RichText::new(Msg::Filters.en())
            .font(theme.title_font(theme.text_display))
            .color(theme.title),
    );

    ui.add_space(12.0);
    scroll::vertical(ui, "torrent-filters", |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        section_label(ui, theme, Msg::FilterQuality.en());
        chips::multi_row(
            ui,
            theme,
            &mut state.filter.quality,
            QualityBand::ALL,
            |band| band.label().to_owned(),
        );

        section_label(ui, theme, Msg::FilterHdr.en());
        tri_row(ui, theme, &mut state.filter.hdr);
        section_label(ui, theme, Msg::FilterDolby.en());
        tri_row(ui, theme, &mut state.filter.dolby);
        section_label(ui, theme, Msg::FilterSubs.en());
        tri_row(ui, theme, &mut state.filter.subs);

        let hits = match &state.hits {
            TorrentHits::Ready(hits) => hits.as_slice(),
            _ => &[],
        };

        let voices = voice_filter_options(hits, &state.filter.voice);
        section_label(ui, theme, Msg::FilterTranslation.en());
        multiselect::show_with(
            ui,
            theme,
            "torrent-voice",
            &mut state.filter.voice,
            &voices,
            |voice| voice_label(voice).to_owned(),
        );

        section_label(ui, theme, Msg::FilterLanguage.en());
        multiselect::show_with(
            ui,
            theme,
            "torrent-lang",
            &mut state.filter.lang,
            AudioLang::ALL,
            |lang| audio_lang_label(lang).to_owned(),
        );

        let years = year_options(hits, state.year, &state.filter.year);
        if years.len() > 1 {
            section_label(ui, theme, Msg::FilterYear.en());
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
                section_label(ui, theme, Msg::Season.en());
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
            state.filter = cinebox_parse::TorrentFilter::default();
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
            if chip(ui, theme, tri_label(*choice), *value == *choice) {
                *value = *choice;
            }
        }
    });
}

fn tri_label(choice: TriChoice) -> &'static str {
    match choice {
        TriChoice::Any => Msg::FilterAny.en(),
        TriChoice::Yes => Msg::Yes.en(),
        TriChoice::No => Msg::No.en(),
    }
}

fn sort_label(mode: SortMode) -> &'static str {
    match mode {
        SortMode::Popular => Msg::SortPopular.en(),
        SortMode::Seeders => Msg::SortSeeders.en(),
        SortMode::Size => Msg::SortSize.en(),
    }
}

fn voice_label(filter: VoiceFilter) -> &'static str {
    match filter {
        VoiceFilter::Dubbing => Msg::VoiceDubbing.en(),
        VoiceFilter::Polyphonic => Msg::VoicePolyphonic.en(),
        VoiceFilter::TwoVoice => Msg::VoiceTwoVoice.en(),
        VoiceFilter::Amateur => Msg::VoiceAmateur.en(),
        VoiceFilter::Studio(name) => name,
    }
}

fn audio_lang_label(lang: AudioLang) -> &'static str {
    match lang {
        AudioLang::Ru => Msg::LangRussian.en(),
        AudioLang::En => Msg::LangEnglish.en(),
        AudioLang::Uk => Msg::LangUkrainian.en(),
        AudioLang::Ja => Msg::LangJapanese.en(),
        AudioLang::Ko => Msg::LangKorean.en(),
        AudioLang::Zh => Msg::LangChinese.en(),
        AudioLang::De => Msg::LangGerman.en(),
        AudioLang::Fr => Msg::LangFrench.en(),
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
        Msg::FilterReset.en(),
        Opts::secondary(vec2(width, 36.0)),
    )
}

fn hit_row(
    ui: &mut Ui,
    hit: &cinebox_parse::TorrentHit,
    kind: MediaKind,
    runtime: Option<u32>,
    theme: &Theme,
    pick: &mut Option<usize>,
    index: usize,
) {
    let id = ui.id().with(("torrent-hit", index));
    let shown = Frame::new()
        .fill(theme.card)
        .corner_radius(theme.rounding(theme.radius_card))
        .inner_margin(egui::Margin::symmetric(12, 14))
        .show(ui, |ui| {
            ui.label(
                RichText::new(typograph(&hit.title))
                    .font(theme.title_font(theme.text_label))
                    .color(theme.title),
            );

            ui.add_space(10.0);
            let bitrate = format_bitrate(kind, hit_bitrate_mbps(hit, runtime));

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let size_label = hit.size_label();
                    pill(
                        ui,
                        theme,
                        &size_label,
                        theme.size_pill_bg,
                        theme.size_pill_fg,
                    );
                    metrics_bar(
                        ui,
                        theme,
                        bitrate.as_deref(),
                        &hit.seeders.to_string(),
                        &hit.peers.to_string(),
                    );

                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let date = if hit.published.is_empty() {
                            "—"
                        } else {
                            hit.published.as_str()
                        };

                        ui.label(
                            RichText::new(date)
                                .size(theme.text_caption)
                                .color(theme.muted),
                        );
                        ui.label(
                            RichText::new(&hit.tracker)
                                .size(theme.text_caption)
                                .color(theme.muted),
                        );

                        if hit.started {
                            ui.label(RichText::new(Msg::TagStarted.en()).color(theme.ok));
                        }
                    });
                });
            });
        });

    let response = button::click_rect(ui, id, shown.response.rect);
    if response.hovered() {
        hit_ring(ui, shown.response.rect, theme);
    }

    if response.clicked() && !hit.magnet.is_empty() {
        *pick = Some(index);
    }
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

const METRIC_VAL_H: f32 = 16.0;
const METRIC_GAP: f32 = 12.0;
const METRIC_LABEL_GAP: f32 = 6.0;

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
    metric_pair(ui, Msg::Leechers.en(), leechers, theme);
    metric_pair(ui, Msg::Seeds.en(), seeds, theme);

    let Some(bitrate) = bitrate else {
        return;
    };

    metric_pair(ui, Msg::Bitrate.en(), bitrate, theme);
}

/// `item_spacing` applies after a widget, so each widget sets the gap that follows it.
fn metric_pair(ui: &mut Ui, label: &str, value: &str, theme: &Theme) {
    ui.spacing_mut().item_spacing.x = METRIC_LABEL_GAP;
    pill(ui, theme, value, theme.metric_bg, theme.title);

    ui.spacing_mut().item_spacing.x = METRIC_GAP;
    ui.label(
        RichText::new(label)
            .size(theme.text_caption)
            .color(theme.muted),
    );
}

/// Fixed `METRIC_VAL_H` content height keeps every pill in the row the same height.
fn pill(ui: &mut Ui, theme: &Theme, text: &str, bg: Color32, fg: Color32) {
    Frame::new()
        .fill(bg)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            let font = theme.ui_font(theme.text_caption);
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
