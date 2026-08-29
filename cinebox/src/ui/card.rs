use cinebox_core::i18n::Msg;
use cinebox_core::{
    CatalogItem, CreditPerson, MediaDetails, MediaKind, PosterSize, TmdbId, Trailer, format_money,
    format_release_date, tmdb_image_url, typograph,
};
use iced::widget::image::Handle as ImageHandle;
use iced::widget::text::Wrapping;
use iced::widget::{Space, button, column, container, mouse_area, row, text};
use iced::{Alignment, Color, ContentFit, Element, Fill, FillPortion, padding};

use crate::app::Message;
use crate::ui::home::{ExtraImages, POSTER_RADIUS, PosterMap, catalog_shelf_height, catalog_tile};
use crate::ui::scroll::{self, ScrollFlash, ScrollPane};

const MUTED: Color = Color::from_rgb(0.65, 0.65, 0.68);
const ERR: Color = Color::from_rgb(0.92, 0.38, 0.38);
const TITLE: Color = Color::from_rgb(0.96, 0.96, 0.97);
const RATE: Color = Color::from_rgb(1.0, 0.85, 0.25);
const BADGE_TEXT: f32 = 20.0;
const BADGE_PAD_Y: f32 = 6.0;
const BADGE_H: f32 = BADGE_PAD_Y * 2.0 + BADGE_TEXT * 1.3;
pub(crate) const POSTER_W: f32 = 200.0;
pub(crate) const POSTER_H: f32 = 300.0;

pub enum MediaState {
    Loading {
        kind: MediaKind,
        id: TmdbId,
    },
    Ready(Box<MediaDetails>),
    Failed {
        kind: MediaKind,
        id: TmdbId,
        error: String,
    },
}

impl MediaState {
    pub fn matches(&self, kind: MediaKind, id: TmdbId) -> bool {
        match self {
            Self::Loading { kind: k, id: i } | Self::Failed { kind: k, id: i, .. } => {
                *k == kind && *i == id
            }
            Self::Ready(details) => details.kind == kind && details.id == id,
        }
    }
}

pub fn view<'a>(
    state: &'a MediaState,
    posters: &'a PosterMap,
    images: &'a ExtraImages,
    poster_size: PosterSize,
    scroll: &'a ScrollFlash,
) -> Element<'a, Message> {
    match state {
        MediaState::Loading { .. } => loading(),
        MediaState::Failed { error, .. } => failed(error),
        MediaState::Ready(details) => ready(details, posters, images, poster_size, scroll),
    }
}

/// Backdrop used as the full-window wallpaper (including chrome).
pub fn wallpaper<'a>(state: &'a MediaState, images: &'a ExtraImages) -> Option<&'a ImageHandle> {
    let MediaState::Ready(details) = state else {
        return None;
    };
    let url = tmdb_image_url(details.backdrop_path.as_deref(), "w1280")?;
    images.get(&url)
}

pub fn loading<'a>() -> Element<'a, Message> {
    status(Msg::LoadingCard.en(), MUTED)
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
            button(text("Retry")).on_press(Message::RetryMedia),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn ready<'a>(
    details: &'a MediaDetails,
    posters: &'a PosterMap,
    images: &'a ExtraImages,
    poster_size: PosterSize,
    scroll: &'a ScrollFlash,
) -> Element<'a, Message> {
    let mut body = column![].spacing(22);
    body = body.push(header(details, posters, images, poster_size));
    if let Some(overview) = details.overview.as_deref() {
        body = body.push(
            container(in_detail(overview))
                .padding(padding::top(20))
                .width(Fill),
        );
    }
    if let Some(facts) = facts_row(details) {
        body = body.push(facts);
    }
    let mut row = 0u8;
    if !details.directors.is_empty() {
        body = body.push(people_row(
            Msg::Directors.en(),
            &details.directors,
            images,
            ScrollPane::Row(row),
            scroll.row(row),
        ));
        row = row.saturating_add(1);
    }
    if !details.cast.is_empty() {
        body = body.push(people_row(
            Msg::Cast.en(),
            &details.cast,
            images,
            ScrollPane::Row(row),
            scroll.row(row),
        ));
        row = row.saturating_add(1);
    }
    if !details.collection.is_empty() {
        body = body.push(shelf(
            Msg::Collection.en(),
            &details.collection,
            posters,
            ScrollPane::Row(row),
            scroll.row(row),
        ));
        row = row.saturating_add(1);
    }
    if !details.recommendations.is_empty() {
        body = body.push(shelf(
            Msg::Recommendations.en(),
            &details.recommendations,
            posters,
            ScrollPane::Row(row),
            scroll.row(row),
        ));
        row = row.saturating_add(1);
    }
    if !details.similar.is_empty() {
        body = body.push(shelf(
            Msg::Similar.en(),
            &details.similar,
            posters,
            ScrollPane::Row(row),
            scroll.row(row),
        ));
    }
    if !details.trailers.is_empty() {
        body = body.push(trailers(&details.trailers));
    }
    container(scroll::vertical(scroll.page(), body.padding([0, 8])))
        .padding(16)
        .width(Fill)
        .height(Fill)
        .style(|_| container::Style {
            background: Some(Color::TRANSPARENT.into()),
            ..container::Style::default()
        })
        .into()
}

fn header<'a>(
    details: &'a MediaDetails,
    posters: &'a PosterMap,
    images: &'a ExtraImages,
    poster_size: PosterSize,
) -> Element<'a, Message> {
    let poster_url = tmdb_image_url(details.poster_path.as_deref(), poster_size.tmdb_path());
    let poster_handle = posters
        .get(&(details.kind, details.id))
        .or_else(|| poster_url.as_deref().and_then(|url| images.get(url)));
    let poster = poster_art(poster_handle, POSTER_W, POSTER_H);

    let mut meta = column![].spacing(10).width(Fill);
    let head = details.head_line();
    if !head.is_empty() {
        meta = meta.push(text(head).size(16).color(MUTED).wrapping(Wrapping::Word));
    }
    meta = meta.push(
        text(typograph(&details.title))
            .size(36)
            .color(TITLE)
            .wrapping(Wrapping::Word),
    );
    if let Some(tagline) = details.tagline.as_deref() {
        meta = meta.push(
            text(typograph(tagline))
                .size(18)
                .color(MUTED)
                .wrapping(Wrapping::Word),
        );
    }
    if let Some(row) = ratings_row(
        details.vote.filter(|v| *v > 0.0),
        details.certification.as_deref(),
    ) {
        meta = meta.push(row);
    }
    if let Some(details_line) = detail_row(details) {
        meta = meta.push(details_line);
    }
    meta = meta.push(button(text(Msg::WatchTorrents.en())).on_press(Message::WatchTorrents));

    row![poster, meta]
        .spacing(28)
        .align_y(Alignment::Start)
        .into()
}

fn ratings_row<'a>(vote: Option<f32>, certification: Option<&str>) -> Option<Element<'a, Message>> {
    let cert = certification.filter(|s| !s.is_empty());
    if vote.is_none() && cert.is_none() {
        return None;
    }
    let mut items = row![].spacing(10).align_y(Alignment::Center);
    if let Some(vote) = vote {
        items = items.push(rate_badge(vote));
    }
    if let Some(label) = cert {
        items = items.push(cert_badge(label));
    }
    Some(items.into())
}

fn rate_badge<'a>(vote: f32) -> Element<'a, Message> {
    rating_pill(
        row![
            text(format!("{vote:.1}")).size(BADGE_TEXT).color(RATE),
            text("TMDB").size(13).color(MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .height(Fill),
    )
}

fn cert_badge<'a>(label: &str) -> Element<'a, Message> {
    rating_pill(text(label.to_owned()).size(BADGE_TEXT).color(TITLE))
}

fn rating_pill<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding([BADGE_PAD_Y, 12.0])
        .center_y(BADGE_H)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.35).into()),
            border: iced::border::rounded(6),
            ..container::Style::default()
        })
        .into()
}

fn detail_row(details: &MediaDetails) -> Option<Element<'_, Message>> {
    let bits = details.detail_bits();
    if bits.is_empty() {
        return None;
    }

    let mut children: Vec<Element<'_, Message>> = Vec::new();
    for (i, bit) in bits.into_iter().enumerate() {
        if i > 0 {
            children.push(text("●").size(11).color(MUTED).into());
        }
        children.push(text(bit).size(16).color(TITLE).into());
    }

    Some(
        iced::widget::Row::with_children(children)
            .spacing(10)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
    )
}

fn facts_row(details: &MediaDetails) -> Option<Element<'_, Message>> {
    let mut cells: Vec<Element<'_, Message>> = Vec::new();
    if let Some(released) = details.released.as_deref() {
        cells.push(fact(Msg::Release.en(), format_release_date(released)));
    } else if let Some(year) = details.year {
        cells.push(fact(Msg::Release.en(), year.to_string()));
    }

    if let Some(budget) = details.budget {
        cells.push(fact(Msg::Budget.en(), format_money(budget)));
    }

    if !details.countries.is_empty() {
        cells.push(fact(Msg::Countries.en(), details.countries.join(", ")));
    }

    if cells.is_empty() {
        return None;
    }

    Some(
        iced::widget::Row::with_children(cells)
            .spacing(32)
            .wrap()
            .into(),
    )
}

fn fact<'a>(label: &'static str, value: String) -> Element<'a, Message> {
    column![
        text(label).size(13).color(MUTED),
        text(value).size(16).color(TITLE).wrapping(Wrapping::Word),
    ]
    .spacing(6)
    .into()
}

fn in_detail(overview: &str) -> Element<'_, Message> {
    let block = column![
        text(Msg::InDetail.en()).size(16).color(TITLE),
        text(typograph(overview))
            .size(15)
            .wrapping(Wrapping::Word)
            .color(Color::from_rgb(0.88, 0.88, 0.9)),
    ]
    .spacing(10)
    .width(FillPortion(2));
    row![block, Space::new().width(FillPortion(1))]
        .width(Fill)
        .into()
}

fn people_row<'a>(
    title: &'static str,
    people: &'a [CreditPerson],
    images: &'a ExtraImages,
    pane: ScrollPane,
    flashing: bool,
) -> Element<'a, Message> {
    let tiles = iced::widget::Row::with_children(people.iter().map(|person| {
        let url = tmdb_image_url(person.profile_path.as_deref(), "w185");
        let photo = poster_art(url.as_deref().and_then(|u| images.get(u)), 100.0, 150.0);
        mouse_area(
            column![
                photo,
                text(typograph(&person.name))
                    .size(12)
                    .width(100.0)
                    .wrapping(Wrapping::Word),
                text(typograph(&person.role))
                    .size(11)
                    .color(MUTED)
                    .width(100.0)
                    .wrapping(Wrapping::Word),
            ]
            .spacing(4)
            .width(100.0),
        )
        .on_release(Message::OpenPerson { id: person.id })
        .into()
    }));
    column![
        text(title).size(16),
        scroll::horizontal(pane, flashing, 220.0, tiles.spacing(12).padding(4)),
    ]
    .spacing(8)
    .into()
}

fn shelf<'a>(
    title: &'static str,
    items: &'a [CatalogItem],
    posters: &'a PosterMap,
    pane: ScrollPane,
    flashing: bool,
) -> Element<'a, Message> {
    let tiles = iced::widget::Row::with_children(
        items
            .iter()
            .map(|item| catalog_tile(item, posters.get(&(item.kind, item.id)))),
    );
    column![
        text(title).size(16),
        scroll::horizontal(
            pane,
            flashing,
            catalog_shelf_height(),
            tiles.spacing(12).padding(8),
        ),
    ]
    .spacing(8)
    .into()
}

fn trailers(items: &[Trailer]) -> Element<'_, Message> {
    let mut col = column![text(Msg::Trailers.en()).size(16)].spacing(8);
    for trailer in items {
        let url = trailer.watch_url();
        col = col.push(button(text(typograph(&trailer.name))).on_press(Message::OpenUrl(url)));
    }
    col.into()
}

pub(crate) fn poster_art<'a>(
    handle: Option<&'a ImageHandle>,
    width: f32,
    height: f32,
) -> Element<'a, Message> {
    match handle {
        Some(handle) => iced::widget::image(handle)
            .width(width)
            .height(height)
            .content_fit(ContentFit::Cover)
            .border_radius(POSTER_RADIUS)
            .into(),
        None => container(text(" ").size(1))
            .width(width)
            .height(height)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.16, 0.16, 0.18).into()),
                border: iced::border::rounded(POSTER_RADIUS),
                ..container::Style::default()
            })
            .into(),
    }
}
