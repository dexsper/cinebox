//! Left pane: poster, title, overview, intro animation.

use cinebox_core::{PosterSize, tmdb_image_url, typograph};
use iced::widget::text::Wrapping;
use iced::widget::{Space, container, row, stack, text};
use iced::{Alignment, Color, Element, Fill, padding};

use crate::app::Message;
use crate::ui::card::{self, POSTER_H, POSTER_W};
use crate::ui::home::{ExtraImages, PosterMap};

use super::list::files_column;
use super::state::{MovieBits, TorrentState};
use super::{CARD_GAP, EXPLORER_POSTER_H, EXPLORER_POSTER_W, LEFT_END, MUTED, PAD, RATE, TITLE};

pub(super) fn view<'a>(
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
    let mut col = iced::widget::column![].spacing(8);
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
