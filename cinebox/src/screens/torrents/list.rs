//! Hit list, sort, and filters.

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaKind, typograph};
use cinebox_parse::{
    AudioLang, QualityBand, SortMode, TriChoice, filtered_hits, hit_bitrate_mbps, season_options,
    voice_filter_options, year_options,
};
use egui::{Align, ComboBox, Frame, Layout, RichText, Sense, Ui, UiBuilder, Vec2};

use super::state::{TorrentHits, TorrentState};
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, scroll};

pub(super) fn list_pane(
    ui: &mut Ui,
    state: &mut TorrentState,
    svc: &Services,
    theme: &Theme,
    retry: &mut bool,
    pick: &mut Option<usize>,
    t: f32,
) {
    if t < 0.22 {
        return;
    }

    ui.horizontal(|ui| {
        combo(ui, "sort", &mut state.sort, SortMode::ALL);
        let filters_label = if state.filter.is_active() {
            format!("{} · on", Msg::Filters.en())
        } else {
            Msg::Filters.en().to_owned()
        };
        if ui
            .selectable_label(
                state.filters_open || state.filter.is_active(),
                filters_label,
            )
            .clicked()
        {
            state.filters_open = !state.filters_open;
        }
    });
    if state.filters_open {
        filters_panel(ui, state, svc, theme);
    }

    match &state.hits {
        TorrentHits::Loading => {
            widgets::page_spinner(ui, theme);
        }
        TorrentHits::Failed(error) => {
            ui.label(RichText::new(error).color(theme.err));
            if ui.button("Retry").clicked() {
                *retry = true;
            }
        }
        TorrentHits::Ready(hits) => {
            let visible: Vec<(usize, &cinebox_parse::TorrentHit)> =
                filtered_hits(hits, state.filter).collect();
            ui.label(
                RichText::new(format!("{} / {}", visible.len(), hits.len()))
                    .size(theme.text_small)
                    .color(theme.label),
            );
            scroll::vertical(ui, "torrent-hits", |ui| {
                if visible.is_empty() {
                    ui.label(RichText::new(Msg::NoTorrents.en()).color(theme.muted));
                    return;
                }

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
        }
    }
}

fn filters_panel(ui: &mut Ui, state: &mut TorrentState, svc: &Services, theme: &Theme) {
    ui.add_space(8.0);
    Frame::new()
        .fill(theme.overlay)
        .corner_radius(8)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(Msg::FilterQuality.en());
            ui.horizontal_wrapped(|ui| {
                let selected = state.filter.quality;
                if chip(ui, "Any", selected.is_none()) {
                    state.filter.quality = None;
                }
                for band in QualityBand::ALL {
                    if chip(ui, band.label(), selected == Some(*band)) {
                        state.filter.quality = Some(*band);
                    }
                }
            });
            ui.label(Msg::FilterHdr.en());
            tri_row(ui, &mut state.filter.hdr);
            ui.label(Msg::FilterDolby.en());
            tri_row(ui, &mut state.filter.dolby);
            ui.label(Msg::FilterSubs.en());
            tri_row(ui, &mut state.filter.subs);

            let hits = match &state.hits {
                TorrentHits::Ready(hits) => hits.as_slice(),
                _ => &[],
            };
            let voices = voice_filter_options(hits, state.filter.voice);
            combo(ui, "voice", &mut state.filter.voice, &voices);
            combo(ui, "lang", &mut state.filter.lang, AudioLang::ALL);

            let years = year_options(hits, state.year, state.filter.year);
            if years.len() > 1 {
                ui.horizontal(|ui| {
                    ui.label(Msg::FilterYear.en());
                    if ui
                        .selectable_label(state.filter.year.is_none(), "Any")
                        .clicked()
                    {
                        state.filter.year = None;
                    }
                    for year in years {
                        if ui
                            .selectable_label(state.filter.year == Some(year), format!("{year}"))
                            .clicked()
                        {
                            state.filter.year = Some(year);
                        }
                    }
                });
            }
            if state.kind == MediaKind::Tv {
                let seasons = season_options(hits);
                if !seasons.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Season");
                        if ui
                            .selectable_label(state.filter.season.is_none(), "Any")
                            .clicked()
                        {
                            state.filter.season = None;
                        }
                        for season in seasons {
                            if ui
                                .selectable_label(
                                    state.filter.season == Some(season),
                                    format!("S{season}"),
                                )
                                .clicked()
                            {
                                state.filter.season = Some(season);
                            }
                        }
                    });
                }
            }
            if ui.button(Msg::FilterReset.en()).clicked() {
                state.filter = cinebox_parse::TorrentFilter::default();
            }
        });
    state.apply_filter_sort(svc.settings.player.default_quality);
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
    let response = Frame::new()
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
                    pill(ui, hit.size_label(), theme);
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
                        ui.label(RichText::new(date).size(theme.text_caption).color(theme.muted));
                        ui.label(RichText::new(&hit.tracker).size(theme.text_caption).color(theme.muted));
                        if hit.started {
                            ui.label(RichText::new(Msg::TagStarted.en()).color(theme.ok));
                        }
                    });
                });
            });
        })
        .response
        .interact(Sense::click());
    if response.clicked() && !hit.magnet.is_empty() {
        *pick = Some(index);
    }
}

fn combo<T: Copy + PartialEq + std::fmt::Display>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    options: &[T],
) {
    ComboBox::from_id_salt(id)
        .selected_text(value.to_string())
        .show_ui(ui, |ui| {
            for opt in options {
                ui.selectable_value(value, *opt, opt.to_string());
            }
        });
}

fn tri_row(ui: &mut Ui, value: &mut TriChoice) {
    ui.horizontal(|ui| {
        for choice in TriChoice::ALL {
            if chip(ui, choice.label(), *value == *choice) {
                *value = *choice;
            }
        }
    });
}

fn chip(ui: &mut Ui, label: &str, active: bool) -> bool {
    ui.selectable_label(active, label).clicked()
}

const METRIC_VAL_H: f32 = 16.0;
const BITRATE_VAL_W: f32 = 36.0;
const COUNT_VAL_W: f32 = 40.0;

pub(super) fn format_bitrate(kind: MediaKind, mbps: Option<f64>) -> Option<String> {
    if kind != MediaKind::Movie {
        return None;
    }

    Some(
        mbps.map(|mbps| format!("{mbps:.1}"))
            .unwrap_or_else(|| String::from("—")),
    )
}

fn metrics_bar(ui: &mut Ui, theme: &Theme, bitrate: Option<&str>, seeds: &str, leechers: &str) {
    Frame::new()
        .fill(theme.metric_bg)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.horizontal(|ui| {
                if let Some(bitrate) = bitrate {
                    metric_pair(ui, Msg::Bitrate.en(), bitrate, BITRATE_VAL_W, theme);
                }
                metric_pair(ui, Msg::Seeds.en(), seeds, COUNT_VAL_W, theme);
                metric_pair(ui, Msg::Leechers.en(), leechers, COUNT_VAL_W, theme);
            });
        });
}

fn metric_pair(ui: &mut Ui, label: &str, value: &str, value_w: f32, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(label).size(theme.text_caption).color(theme.muted));
        let (rect, _) = ui.allocate_exact_size(Vec2::new(value_w, METRIC_VAL_H), Sense::hover());
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.label(RichText::new(value).size(theme.text_caption).color(theme.title));
            },
        );
    });
}

fn pill(ui: &mut Ui, label: String, theme: &Theme) {
    Frame::new()
        .fill(theme.size_pill_bg)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(theme.text_caption).color(theme.size_pill_fg));
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
