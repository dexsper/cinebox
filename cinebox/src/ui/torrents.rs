//! Explorer-style torrent list: movie card on the left, results on the right.

use cinebox_core::i18n::Msg;
use cinebox_core::{
    DefaultQuality, MediaDetails, MediaKind, PosterSize, TmdbId, tmdb_image_url, typograph,
};
use cinebox_parse::{
    AudioLang, QualityBand, SortMode, TorrentFilter, TorrentHit, TriChoice, VoiceFilter,
    matches_filter, sort_hits, studios_in_catalog_order,
};
use iced::widget::text::Wrapping;
use iced::widget::{Space, button, column, container, grid, pick_list, row, stack, text};
use iced::{Alignment, Color, Element, Fill, Length, padding};

use crate::app::Message;
use crate::ui::card::{self, POSTER_H, POSTER_W};
use crate::ui::home::{ExtraImages, PosterMap};
use crate::ui::scroll;

const MUTED: Color = Color::from_rgb(0.78, 0.78, 0.82);
const LABEL: Color = Color::from_rgb(0.92, 0.92, 0.94);
const ERR: Color = Color::from_rgb(0.92, 0.38, 0.38);
const TITLE: Color = Color::from_rgb(0.96, 0.96, 0.97);
const RATE: Color = Color::from_rgb(1.0, 0.85, 0.25);
const PAD: f32 = 16.0;
const FILES_GUTTER: f32 = 16.0;
const CARD_GAP: f32 = 28.0;
const EXPLORER_POSTER_W: f32 = 112.0;
const EXPLORER_POSTER_H: f32 = 168.0;
const LEFT_END: f32 = 340.0;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    ToggleFilters,
    FilterQuality(Option<QualityBand>),
    FilterHdr(TriChoice),
    FilterDolby(TriChoice),
    FilterSubs(TriChoice),
    FilterVoice(VoiceFilter),
    FilterLang(AudioLang),
    FilterSeason(Option<u32>),
    FilterYear(Option<u16>),
    FilterReset,
    Sort(SortMode),
    Pick(usize),
}

#[derive(Debug, Clone)]
pub struct MovieBits {
    pub title: String,
    pub overview: Option<String>,
    pub year: Option<u16>,
    pub vote: Option<f32>,
    pub genres: Vec<String>,
    pub countries: Vec<String>,
    pub poster_path: Option<String>,
}

impl MovieBits {
    pub fn from_details(details: &MediaDetails) -> Self {
        Self {
            title: details.title.clone(),
            overview: details.overview.clone(),
            year: details.year,
            vote: details.vote,
            genres: details.genres.iter().take(3).cloned().collect(),
            countries: details.countries.clone(),
            poster_path: details.poster_path.clone(),
        }
    }

    fn head_line(&self) -> String {
        let mut parts = Vec::new();
        if let Some(year) = self.year {
            parts.push(year.to_string());
        }
        if let Some(country) = self.countries.first() {
            parts.push(country.clone());
        }
        parts.join(" - ")
    }
}

#[derive(Debug, Clone)]
pub enum TorrentHits {
    Loading,
    Ready(Vec<TorrentHit>),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TorrentState {
    pub kind: MediaKind,
    pub id: TmdbId,
    pub movie: MovieBits,
    pub year: Option<u16>,
    pub hits: TorrentHits,
    pub filter: TorrentFilter,
    pub sort: SortMode,
    pub filters_open: bool,
    pub pick_hint: bool,
}

impl TorrentState {
    pub fn from_details(details: &MediaDetails) -> Self {
        Self {
            kind: details.kind,
            id: details.id,
            movie: MovieBits::from_details(details),
            year: details.year,
            hits: TorrentHits::Loading,
            filter: TorrentFilter::default(),
            sort: SortMode::Popular,
            filters_open: false,
            pick_hint: false,
        }
    }

    pub fn matches(&self, kind: MediaKind, id: TmdbId) -> bool {
        self.kind == kind && self.id == id
    }
}

pub fn update(state: &mut TorrentState, event: Event, preferred: DefaultQuality) {
    match event {
        Event::ToggleFilters => state.filters_open = !state.filters_open,
        Event::FilterQuality(quality) => state.filter.quality = quality,
        Event::FilterHdr(choice) => state.filter.hdr = choice,
        Event::FilterDolby(choice) => state.filter.dolby = choice,
        Event::FilterSubs(choice) => state.filter.subs = choice,
        Event::FilterVoice(voice) => state.filter.voice = voice,
        Event::FilterLang(lang) => state.filter.lang = lang,
        Event::FilterSeason(season) => state.filter.season = season,
        Event::FilterYear(year) => state.filter.year = year,
        Event::FilterReset => state.filter = TorrentFilter::default(),
        Event::Sort(mode) => {
            state.sort = mode;
            if let TorrentHits::Ready(hits) = &mut state.hits {
                sort_hits(hits, state.kind, preferred, mode);
            }
        }
        Event::Pick(_) => state.pick_hint = true,
    }
}

pub fn view<'a>(
    state: &'a TorrentState,
    posters: &'a PosterMap,
    images: &'a ExtraImages,
    poster_size: PosterSize,
    intro: f32,
    flashing: bool,
) -> Element<'a, Message> {
    let t = intro.clamp(0.0, 1.0);
    let poster_w = lerp(POSTER_W, EXPLORER_POSTER_W, t);
    let poster_h = lerp(POSTER_H, EXPLORER_POSTER_H, t);
    let left_w = lerp(PAD + POSTER_W + 8.0, LEFT_END, t);

    let poster_url = tmdb_image_url(state.movie.poster_path.as_deref(), poster_size.tmdb_path());
    let poster_handle = posters
        .get(&(state.kind, state.id))
        .or_else(|| poster_url.as_deref().and_then(|url| images.get(url)));

    let title_x = lerp(PAD + POSTER_W + CARD_GAP, PAD, t);
    let title_y = lerp(0.0, poster_h + 18.0, t);
    let title_w = lerp(720.0, LEFT_END - PAD * 2.0, t);
    let title_size = lerp(32.0, 22.0, t);
    let genres_y = title_y + lerp(44.0, 52.0, t);
    let overview_y = lerp(POSTER_H + 22.0, genres_y + 32.0, t);
    let overview_w = lerp(780.0, LEFT_END - PAD * 2.0, t);
    let meta_x = PAD + poster_w + 14.0;
    let files_alpha = ((t - 0.22) / 0.35).clamp(0.0, 1.0);

    let files: Element<'a, Message> = if files_alpha <= 0.02 {
        Space::new().width(Fill).height(Fill).into()
    } else {
        files_column(state, flashing)
    };

    let mut layers: Vec<Element<'a, Message>> = vec![
        row![Space::new().width(left_w).height(Fill), files]
            .width(Fill)
            .height(Fill)
            .into(),
        at(
            lerp(PAD, PAD, t),
            0.0,
            card::poster_art(poster_handle, poster_w, poster_h),
        ),
    ];

    if t > 0.2 {
        layers.push(at(meta_x, 6.0, side_meta(&state.movie, t)));
    }
    layers.push(at(
        title_x,
        title_y,
        text(typograph(&state.movie.title))
            .size(title_size)
            .color(TITLE)
            .width(title_w)
            .wrapping(Wrapping::Word)
            .into(),
    ));
    if !state.movie.genres.is_empty() && t > 0.25 {
        layers.push(at(
            title_x,
            genres_y,
            text(state.movie.genres.join(", "))
                .size(13)
                .color(fade(MUTED, ((t - 0.25) / 0.5).clamp(0.0, 1.0)))
                .width(title_w)
                .wrapping(Wrapping::Word)
                .into(),
        ));
    }
    if let Some(overview) = state.movie.overview.as_deref() {
        layers.push(at(
            PAD,
            overview_y,
            text(typograph(overview))
                .size(lerp(15.0, 13.5, t))
                .color(Color::from_rgb(0.88, 0.88, 0.9))
                .width(overview_w)
                .wrapping(Wrapping::Word)
                .into(),
        ));
    }

    stack(layers).width(Fill).height(Fill).into()
}

fn side_meta(movie: &MovieBits, t: f32) -> Element<'_, Message> {
    let alpha = ((t - 0.2) / 0.5).clamp(0.0, 1.0);
    let mut col = column![].spacing(8);
    let head = movie.head_line();
    if !head.is_empty() {
        col = col.push(text(head).size(13).color(fade(MUTED, alpha)));
    }
    if let Some(vote) = movie.vote.filter(|v| *v > 0.0) {
        col = col.push(
            row![
                text(format!("{vote:.1}")).size(18).color(fade(RATE, alpha)),
                text("TMDB").size(12).color(fade(MUTED, alpha)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    col.into()
}

pub fn loading<'a>() -> Element<'a, Message> {
    container(text(Msg::LoadingTorrents.en()).color(MUTED))
        .padding(16)
        .width(Fill)
        .height(Fill)
        .into()
}

fn files_column<'a>(state: &'a TorrentState, flashing: bool) -> Element<'a, Message> {
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
    let mut choices = VoiceFilter::KINDS.to_vec();
    if let TorrentHits::Ready(hits) = &state.hits {
        let studios =
            studios_in_catalog_order(hits.iter().flat_map(|hit| hit.voices.iter().copied()));
        choices.extend(studios.into_iter().map(VoiceFilter::Studio));
    }
    if let VoiceFilter::Studio(name) = state.filter.voice
        && !choices.contains(&VoiceFilter::Studio(name))
    {
        choices.push(VoiceFilter::Studio(name));
    }
    choices
}

fn list_body<'a>(
    state: &'a TorrentState,
    hits: &'a [TorrentHit],
    flashing: bool,
) -> Element<'a, Message> {
    let visible: Vec<(usize, &TorrentHit)> = hits
        .iter()
        .enumerate()
        .filter(|(_, hit)| matches_filter(hit, state.filter))
        .collect();

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
    } else {
        for (index, hit) in visible {
            col = col.push(hit_row(index, hit));
        }
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

fn hit_row(index: usize, hit: &TorrentHit) -> Element<'_, Message> {
    let date = if hit.published.is_empty() {
        String::from("—")
    } else {
        hit.published.clone()
    };
    let trackers = hit
        .tracker
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let bitrate = match hit.bitrate_mbps {
        Some(mbps) => format!("{mbps:.1}"),
        None => String::from("—"),
    };
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

    let right = row![
        stat(Msg::Bitrate.en(), bitrate),
        stat(Msg::Seeds.en(), hit.seeders.to_string()),
        stat(Msg::Leechers.en(), hit.peers.to_string()),
        size_pill(hit.size_label()),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

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
        card.into()
    } else {
        button(card)
            .on_press(Message::Torrents(Event::Pick(index)))
            .padding(0)
            .style(button::text)
            .width(Fill)
            .into()
    }
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
    let mut years = vec![YearChoice::Any];
    if let Some(year) = state.year {
        years.push(YearChoice::Year(year));
    }
    if let TorrentHits::Ready(hits) = &state.hits {
        let mut extra: Vec<u16> = hits.iter().filter_map(|hit| hit.info.year).collect();
        extra.sort_unstable();
        extra.reverse();
        extra.dedup();
        for year in extra.into_iter().take(8) {
            let choice = YearChoice::Year(year);
            if !years.contains(&choice) {
                years.push(choice);
            }
        }
    }
    if let Some(year) = state.filter.year {
        let choice = YearChoice::Year(year);
        if !years.contains(&choice) {
            years.push(choice);
        }
    }
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
    let mut seasons: Vec<u32> = hits
        .iter()
        .flat_map(|hit| hit.info.seasons.iter().copied())
        .collect();
    seasons.sort_unstable();
    seasons.dedup();
    if seasons.is_empty() {
        return None;
    }
    let mut choices = vec![SeasonChoice::Any];
    choices.extend(seasons.into_iter().map(SeasonChoice::Season));
    Some(choices)
}

fn at<'a>(x: f32, y: f32, child: Element<'a, Message>) -> Element<'a, Message> {
    container(child)
        .padding(padding::left(x.max(0.0)).top(y.max(0.0)))
        .width(Fill)
        .height(Fill)
        .into()
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn fade(color: Color, t: f32) -> Color {
    Color {
        a: color.a * t.clamp(0.0, 1.0),
        ..color
    }
}
