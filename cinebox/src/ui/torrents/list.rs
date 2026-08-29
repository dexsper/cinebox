//! Right pane: filters, sort, torrent rows.

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaKind, typograph};
use cinebox_parse::{
    AudioLang, QualityBand, SortMode, TorrentHit, TriChoice, VoiceFilter, filtered_hits,
    hit_bitrate_mbps, season_options, voice_filter_options, year_options,
};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, grid, pick_list, row, text};
use iced::{Alignment, Color, Element, Fill, Length, padding};

use crate::app::Message;
use crate::ui::scroll;

use super::state::{Event, TorrentHits, TorrentState};
use super::{ERR, FILES_GUTTER, LABEL, MUTED, TITLE};

pub(super) fn files_column<'a>(state: &'a TorrentState, flashing: bool) -> Element<'a, Message> {
    let filters_label = if state.filter.is_active() {
        format!("{} · on", Msg::Filters.en())
    } else {
        Msg::Filters.en().to_owned()
    };

    let mut head = column![
        row![
            pick_list(SortMode::ALL, Some(state.sort), |mode| Message::Torrents(
                Event::Sort(mode)
            ),)
            .placeholder(Msg::Sort.en())
            .padding([5, 10])
            .text_size(14),
            button(text(filters_label).size(14))
                .on_press(Message::Torrents(Event::ToggleFilters))
                .padding([5, 10])
                .style(if state.filters_open || state.filter.is_active() {
                    button::primary
                } else {
                    button::secondary
                }),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(10)
    .width(Fill);

    if state.filters_open {
        head = head.push(filters_panel(state));
    }

    if state.pick_hint {
        head = head.push(text(Msg::PickSoon.en()).size(13).color(MUTED));
    }

    let body = match &state.hits {
        TorrentHits::Loading => status(Msg::LoadingTorrents.en(), LABEL),
        TorrentHits::Failed(error) => failed(error),
        TorrentHits::Ready(hits) => list_body(state, hits, flashing),
    };

    column![
        container(head)
            .padding(
                padding::top(8)
                    .right(FILES_GUTTER)
                    .bottom(12)
                    .left(FILES_GUTTER)
            )
            .width(Fill),
        container(body).width(Fill).height(Fill),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

fn filters_panel(state: &TorrentState) -> Element<'_, Message> {
    let quality = QualityChoice::from_filter(state.filter.quality);
    let mut cells = vec![
        filter_group(
            Msg::FilterQuality.en(),
            chips(QualityChoice::ALL.iter().copied().map(|choice| {
                chip(
                    choice.to_string(),
                    choice == quality,
                    Message::Torrents(Event::FilterQuality(choice.to_filter())),
                )
            })),
        ),
        filter_group(
            Msg::FilterHdr.en(),
            tri_chips(state.filter.hdr, |choice| {
                Message::Torrents(Event::FilterHdr(choice))
            }),
        ),
        filter_group(
            Msg::FilterDolby.en(),
            tri_chips(state.filter.dolby, |choice| {
                Message::Torrents(Event::FilterDolby(choice))
            }),
        ),
        filter_group(
            Msg::FilterSubs.en(),
            tri_chips(state.filter.subs, |choice| {
                Message::Torrents(Event::FilterSubs(choice))
            }),
        ),
        filter_group(
            Msg::FilterTranslation.en(),
            pick_list(
                translation_choices(state),
                Some(state.filter.voice),
                |choice| Message::Torrents(Event::FilterVoice(choice)),
            )
            .width(Fill)
            .into(),
        ),
        filter_group(
            Msg::FilterLanguage.en(),
            pick_list(AudioLang::ALL, Some(state.filter.lang), |choice| {
                Message::Torrents(Event::FilterLang(choice))
            })
            .width(Fill)
            .into(),
        ),
    ];

    let years = year_choices(state);
    if years.len() > 1 {
        let selected = match state.filter.year {
            Some(year) => YearChoice::Year(year),
            None => YearChoice::Any,
        };
        cells.push(filter_group(
            Msg::FilterYear.en(),
            pick_list(years, Some(selected), |choice| {
                Message::Torrents(Event::FilterYear(choice.to_filter()))
            })
            .width(Fill)
            .into(),
        ));
    }

    if let Some(seasons) = season_choices(state) {
        let selected = match state.filter.season {
            Some(season) => SeasonChoice::Season(season),
            None => SeasonChoice::Any,
        };
        cells.push(filter_group(
            "Season",
            pick_list(seasons, Some(selected), |choice| {
                Message::Torrents(Event::FilterSeason(choice.to_filter()))
            })
            .width(Fill)
            .into(),
        ));
    }

    container(
        column![
            grid(cells).spacing(12).fluid(220.0).height(Length::Shrink),
            button(text(Msg::FilterReset.en()).size(13))
                .on_press(Message::Torrents(Event::FilterReset))
                .padding([5, 12])
                .style(button::secondary),
        ]
        .spacing(10)
        .width(Fill),
    )
    .padding(12)
    .width(Fill)
    .style(|_| container::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.32).into()),
        border: iced::border::rounded(8),
        ..container::Style::default()
    })
    .into()
}

fn filter_group<'a>(label: &'static str, control: Element<'a, Message>) -> Element<'a, Message> {
    column![text(label).size(12).color(LABEL), control]
        .spacing(6)
        .width(Fill)
        .into()
}

fn tri_chips(selected: TriChoice, to_msg: fn(TriChoice) -> Message) -> Element<'static, Message> {
    chips(
        TriChoice::ALL
            .iter()
            .copied()
            .map(|choice| chip(choice.label(), choice == selected, to_msg(choice))),
    )
}

fn chips<'a>(items: impl IntoIterator<Item = Element<'a, Message>>) -> Element<'a, Message> {
    row(items).spacing(6).wrap().vertical_spacing(6).into()
}

fn chip<'a>(label: impl Into<String>, active: bool, on_press: Message) -> Element<'a, Message> {
    button(text(label.into()).size(12))
        .on_press(on_press)
        .padding([5, 10])
        .style(if active {
            button::primary
        } else {
            button::secondary
        })
        .into()
}

fn translation_choices(state: &TorrentState) -> Vec<VoiceFilter> {
    let hits = match &state.hits {
        TorrentHits::Ready(hits) => hits.as_slice(),
        _ => &[],
    };
    voice_filter_options(hits, state.filter.voice)
}

fn list_body<'a>(
    state: &'a TorrentState,
    hits: &'a [TorrentHit],
    flashing: bool,
) -> Element<'a, Message> {
    let visible: Vec<(usize, &TorrentHit)> = filtered_hits(hits, state.filter).collect();

    let mut col = column![]
        .spacing(10)
        .padding(padding::right(FILES_GUTTER).bottom(16).left(FILES_GUTTER))
        .width(Fill);
    col = col.push(
        text(format!("{} / {}", visible.len(), hits.len()))
            .size(13)
            .color(LABEL),
    );

    if visible.is_empty() {
        col = col.push(text(Msg::NoTorrents.en()).size(14).color(LABEL));
        return scroll::vertical(flashing, col);
    }

    for (index, hit) in visible {
        col = col.push(hit_row(index, hit, state.kind, state.runtime_minutes));
    }
    scroll::vertical(flashing, col)
}

fn status(message: &'static str, color: Color) -> Element<'static, Message> {
    container(text(message).color(color))
        .padding(16)
        .width(Fill)
        .height(Fill)
        .into()
}

fn failed(error: &str) -> Element<'static, Message> {
    container(
        column![
            text(error.to_owned()).size(14).color(ERR),
            button(text("Retry")).on_press(Message::RetryTorrents),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn hit_row(
    index: usize,
    hit: &TorrentHit,
    kind: MediaKind,
    runtime_minutes: Option<u32>,
) -> Element<'_, Message> {
    let date = if hit.published.is_empty() {
        "—"
    } else {
        hit.published.as_str()
    };

    let trackers = hit
        .tracker
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    let left = row![
        text(date).size(12).color(MUTED),
        text(trackers)
            .size(12)
            .color(MUTED)
            .wrapping(Wrapping::Word),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .width(Fill);

    let mut stats: Vec<Element<'_, Message>> = Vec::new();
    if kind == MediaKind::Movie
        && let Some(mbps) = hit_bitrate_mbps(hit, runtime_minutes)
    {
        stats.push(stat(Msg::Bitrate.en(), format!("{mbps:.1}")));
    }

    stats.push(stat(Msg::Seeds.en(), hit.seeders.to_string()));
    stats.push(stat(Msg::Leechers.en(), hit.peers.to_string()));
    stats.push(size_pill(hit.size_label()));
    let right = row(stats).spacing(12).align_y(Alignment::Center);

    let inner = column![
        text(typograph(&hit.title))
            .size(15)
            .color(TITLE)
            .wrapping(Wrapping::Word),
        row![left, right]
            .spacing(12)
            .align_y(Alignment::Center)
            .width(Fill),
    ]
    .spacing(8);

    let card = container(inner)
        .padding(12)
        .width(Fill)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.08, 0.08, 0.1, 0.72).into()),
            border: iced::border::rounded(8),
            ..container::Style::default()
        });

    if hit.magnet.is_empty() {
        return card.into();
    }
    button(card)
        .on_press(Message::Torrents(Event::Pick(index)))
        .padding(0)
        .style(button::text)
        .width(Fill)
        .into()
}

fn stat(label: &'static str, value: String) -> Element<'static, Message> {
    row![
        text(format!("{label}:")).size(12).color(MUTED),
        container(text(value).size(12).color(TITLE))
            .padding([2, 6])
            .style(|_| container::Style {
                background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.16).into()),
                border: iced::border::rounded(4),
                ..container::Style::default()
            }),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn size_pill(label: String) -> Element<'static, Message> {
    container(text(label).size(12).color(Color::BLACK))
        .padding([3, 8])
        .style(|_| container::Style {
            background: Some(Color::WHITE.into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        })
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QualityChoice {
    Any,
    Uhd,
    Fhd,
    Hd,
}

impl QualityChoice {
    const ALL: &[Self] = &[Self::Any, Self::Uhd, Self::Fhd, Self::Hd];

    fn from_filter(quality: Option<QualityBand>) -> Self {
        match quality {
            None => Self::Any,
            Some(QualityBand::Uhd) => Self::Uhd,
            Some(QualityBand::Fhd) => Self::Fhd,
            Some(QualityBand::Hd) => Self::Hd,
        }
    }

    fn to_filter(self) -> Option<QualityBand> {
        match self {
            Self::Any => None,
            Self::Uhd => Some(QualityBand::Uhd),
            Self::Fhd => Some(QualityBand::Fhd),
            Self::Hd => Some(QualityBand::Hd),
        }
    }
}

impl std::fmt::Display for QualityChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Any => "Any",
            Self::Uhd => "4K",
            Self::Fhd => "1080p",
            Self::Hd => "720p",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YearChoice {
    Any,
    Year(u16),
}

impl YearChoice {
    fn to_filter(self) -> Option<u16> {
        match self {
            Self::Any => None,
            Self::Year(year) => Some(year),
        }
    }
}

impl std::fmt::Display for YearChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => f.write_str("Any"),
            Self::Year(year) => write!(f, "{year}"),
        }
    }
}

fn year_choices(state: &TorrentState) -> Vec<YearChoice> {
    let hits = match &state.hits {
        TorrentHits::Ready(hits) => hits.as_slice(),
        _ => &[],
    };

    let mut years = vec![YearChoice::Any];
    years.extend(
        year_options(hits, state.year, state.filter.year)
            .into_iter()
            .map(YearChoice::Year),
    );
    years
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeasonChoice {
    Any,
    Season(u32),
}

impl SeasonChoice {
    fn to_filter(self) -> Option<u32> {
        match self {
            Self::Any => None,
            Self::Season(season) => Some(season),
        }
    }
}

impl std::fmt::Display for SeasonChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => f.write_str("Any"),
            Self::Season(season) => write!(f, "S{season}"),
        }
    }
}

fn season_choices(state: &TorrentState) -> Option<Vec<SeasonChoice>> {
    if state.kind != MediaKind::Tv {
        return None;
    }

    let TorrentHits::Ready(hits) = &state.hits else {
        return None;
    };

    let seasons = season_options(hits);
    if seasons.is_empty() {
        return None;
    }

    let mut choices = vec![SeasonChoice::Any];
    choices.extend(seasons.into_iter().map(SeasonChoice::Season));
    Some(choices)
}
