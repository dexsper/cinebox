//! Explorer-style torrent list: movie card on the left, results on the right.

mod explorer;
mod list;
mod state;

use iced::widget::{container, text};
use iced::{Color, Element, Fill};

use crate::app::Message;
use crate::ui::home::{ExtraImages, PosterMap};

use cinebox_core::PosterSize;

pub use state::{Event, TorrentHits, TorrentState, update};

pub(super) const MUTED: Color = Color::from_rgb(0.78, 0.78, 0.82);
pub(super) const LABEL: Color = Color::from_rgb(0.92, 0.92, 0.94);
pub(super) const ERR: Color = Color::from_rgb(0.92, 0.38, 0.38);
pub(super) const TITLE: Color = Color::from_rgb(0.96, 0.96, 0.97);
pub(super) const RATE: Color = Color::from_rgb(1.0, 0.85, 0.25);
pub(super) const PAD: f32 = 16.0;
pub(super) const FILES_GUTTER: f32 = 16.0;
pub(super) const CARD_GAP: f32 = 28.0;
pub(super) const EXPLORER_POSTER_W: f32 = 112.0;
pub(super) const EXPLORER_POSTER_H: f32 = 168.0;
pub(super) const LEFT_END: f32 = 340.0;

pub fn view<'a>(
    state: &'a TorrentState,
    posters: &'a PosterMap,
    images: &'a ExtraImages,
    poster_size: PosterSize,
    intro: f32,
    flashing: bool,
) -> Element<'a, Message> {
    explorer::view(state, posters, images, poster_size, intro, flashing)
}

pub fn loading<'a>() -> Element<'a, Message> {
    container(text(cinebox_core::i18n::Msg::LoadingTorrents.en()).color(MUTED))
        .padding(16)
        .width(Fill)
        .height(Fill)
        .into()
}
