use std::collections::HashMap;

use cinebox_core::i18n::Msg;
use cinebox_core::{CatalogItem, HomeCatalog, HomeRow, MediaKind, TmdbId, typograph};
use iced::widget::image::Handle as ImageHandle;
use iced::widget::text::Wrapping;
use iced::widget::{Row, button, column, container, scrollable, stack, text};
use iced::{Alignment, Color, ContentFit, Element, Fill};

use crate::app::Message;

const MUTED: Color = Color::from_rgb(0.65, 0.65, 0.68);
const ERR: Color = Color::from_rgb(0.92, 0.38, 0.38);
const TILE_WIDTH: f32 = 140.0;
const POSTER_HEIGHT: f32 = 210.0;

pub type PosterMap = HashMap<(MediaKind, TmdbId), ImageHandle>;

pub enum HomeState {
    NeedKey,
    Loading,
    Ready(HomeCatalog),
    Failed(String),
}

pub fn view<'a>(state: &'a HomeState, posters: &'a PosterMap) -> Element<'a, Message> {
    match state {
        HomeState::NeedKey => need_key(),
        HomeState::Loading => centered_status(Msg::LoadingHome.en()),
        HomeState::Failed(error) => failed(error),
        HomeState::Ready(catalog) => catalog_view(catalog, posters),
    }
}

fn need_key() -> Element<'static, Message> {
    container(
        column![
            text(Msg::HomeTitle.en()).size(20),
            text(Msg::NeedTmdbKey.en()).size(14).color(MUTED),
            button(text(Msg::NavSettings.en())).on_press(Message::OpenSettings),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn centered_status(message: &'static str) -> Element<'static, Message> {
    container(
        column![
            text(Msg::HomeTitle.en()).size(20),
            text(message).color(MUTED)
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn failed(error: &str) -> Element<'static, Message> {
    container(
        column![
            text(Msg::HomeTitle.en()).size(20),
            text(error.to_owned()).size(14).color(ERR),
            button(text("Retry")).on_press(Message::RetryHome),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn catalog_view<'a>(catalog: &'a HomeCatalog, posters: &'a PosterMap) -> Element<'a, Message> {
    let mut body = column![text(Msg::HomeTitle.en()).size(20)].spacing(20);
    for row in &catalog.rows {
        body = body.push(shelf(row, posters));
    }
    container(scrollable(body.padding([0, 8])).width(Fill).height(Fill))
        .padding(16)
        .width(Fill)
        .height(Fill)
        .into()
}

fn shelf<'a>(row: &'a HomeRow, posters: &'a PosterMap) -> Element<'a, Message> {
    let mut block = column![text(row.id.title()).size(16)].spacing(8);
    if let Some(error) = &row.error {
        block = block.push(text(error.clone()).size(13).color(ERR));
    }
    if row.items.is_empty() && row.error.is_none() {
        block = block.push(text(Msg::EmptyRow.en()).size(13).color(MUTED));
    } else if !row.items.is_empty() {
        let tiles = Row::with_children(
            row.items
                .iter()
                .map(|item| tile(item, posters.get(&(item.kind, item.id)))),
        );
        block = block.push(
            scrollable(tiles.spacing(12).padding(4))
                .horizontal()
                .width(Fill)
                .height(POSTER_HEIGHT + 72.0),
        );
    }
    block.into()
}

fn tile<'a>(item: &'a CatalogItem, poster: Option<&'a ImageHandle>) -> Element<'a, Message> {
    let poster = poster_block(poster, item.vote);
    let year = item
        .year
        .map(|year| year.to_string())
        .unwrap_or_else(|| String::from("—"));
    column![
        poster,
        text(typograph(&item.title))
            .size(13)
            .width(TILE_WIDTH)
            .wrapping(Wrapping::Word),
        text(year)
            .size(12)
            .color(MUTED)
            .width(TILE_WIDTH)
            .align_x(Alignment::Start),
    ]
    .spacing(4)
    .width(TILE_WIDTH)
    .align_x(Alignment::Start)
    .into()
}

fn poster_block<'a>(poster: Option<&'a ImageHandle>, vote: Option<f32>) -> Element<'a, Message> {
    let art: Element<'a, Message> = match poster {
        Some(handle) => iced::widget::image(handle)
            .width(TILE_WIDTH)
            .height(POSTER_HEIGHT)
            .content_fit(ContentFit::Cover)
            .into(),
        None => container(text(" ").size(1))
            .width(TILE_WIDTH)
            .height(POSTER_HEIGHT)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.16, 0.16, 0.18).into()),
                ..container::Style::default()
            })
            .into(),
    };

    let mut layers = vec![art];
    if let Some(vote) = vote {
        layers.push(
            container(vote_badge(vote))
                .width(Fill)
                .height(Fill)
                .padding(6)
                .align_x(Alignment::End)
                .align_y(Alignment::End)
                .into(),
        );
    }
    stack(layers).width(TILE_WIDTH).height(POSTER_HEIGHT).into()
}

fn vote_badge<'a>(vote: f32) -> Element<'a, Message> {
    container(text(format!("{vote:.1}")).size(12))
        .padding([2, 6])
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.75).into()),
            text_color: Some(Color::from_rgb(1.0, 0.85, 0.25)),
            border: iced::border::rounded(4),
            ..container::Style::default()
        })
        .into()
}
