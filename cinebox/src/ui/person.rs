use cinebox_core::i18n::Msg;
use cinebox_core::{PersonDetails, TmdbId, tmdb_image_url, typograph};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, grid, row, text};
use iced::{Color, ContentFit, Element, Fill, Length};

use crate::app::Message;
use crate::ui::home::{ExtraImages, POSTER_RADIUS, PosterMap, catalog_tile};
use crate::ui::scroll;

const MUTED: Color = Color::from_rgb(0.65, 0.65, 0.68);
const ERR: Color = Color::from_rgb(0.92, 0.38, 0.38);

pub enum PersonState {
    Loading { id: TmdbId },
    Ready(Box<PersonDetails>),
    Failed { id: TmdbId, error: String },
}

impl PersonState {
    pub fn matches(&self, id: TmdbId) -> bool {
        match self {
            Self::Loading { id: i } | Self::Failed { id: i, .. } => *i == id,
            Self::Ready(details) => details.id == id,
        }
    }
}

pub fn view<'a>(
    state: &'a PersonState,
    posters: &'a PosterMap,
    images: &'a ExtraImages,
    page_flashing: bool,
) -> Element<'a, Message> {
    match state {
        PersonState::Loading { .. } => loading(),
        PersonState::Failed { error, .. } => failed(error),
        PersonState::Ready(details) => ready(details, posters, images, page_flashing),
    }
}

pub fn loading<'a>() -> Element<'a, Message> {
    container(text(Msg::LoadingCard.en()).color(MUTED))
        .padding(16)
        .width(Fill)
        .height(Fill)
        .into()
}

fn failed(error: &str) -> Element<'static, Message> {
    container(
        column![
            text(error.to_owned()).size(14).color(ERR),
            button(text("Retry")).on_press(Message::RetryPerson),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn ready<'a>(
    details: &'a PersonDetails,
    posters: &'a PosterMap,
    images: &'a ExtraImages,
    page_flashing: bool,
) -> Element<'a, Message> {
    let url = tmdb_image_url(details.profile_path.as_deref(), "w185");
    let photo: Element<'a, Message> = match url.as_deref().and_then(|u| images.get(u)) {
        Some(handle) => iced::widget::image(handle)
            .width(140.0)
            .height(210.0)
            .content_fit(ContentFit::Cover)
            .border_radius(POSTER_RADIUS)
            .into(),
        None => container(text(" ").size(1))
            .width(140.0)
            .height(210.0)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.16, 0.16, 0.18).into()),
                border: iced::border::rounded(POSTER_RADIUS),
                ..container::Style::default()
            })
            .into(),
    };
    let mut meta = column![text(typograph(&details.name)).size(26)].spacing(6);
    if let Some(born) = details.birthday.as_deref() {
        meta = meta.push(text(born).size(13).color(MUTED));
    }
    if let Some(place) = details.place_of_birth.as_deref() {
        meta = meta.push(text(typograph(place)).size(13).color(MUTED));
    }
    let mut body = column![row![photo, meta].spacing(16)].spacing(16);
    if let Some(bio) = details.biography.as_deref() {
        body = body.push(text(typograph(bio)).size(14).wrapping(Wrapping::Word));
    }
    if !details.credits.is_empty() {
        body = body.push(text(Msg::Credits.en()).size(16));
        let cells = details
            .credits
            .iter()
            .map(|item| catalog_tile(item, posters.get(&(item.kind, item.id))));
        body = body.push(grid(cells).spacing(12).fluid(160.0).height(Length::Shrink));
    }
    container(scroll::vertical(page_flashing, body.padding([0, 8])))
        .padding(16)
        .width(Fill)
        .height(Fill)
        .into()
}
