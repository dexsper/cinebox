//! File / episode list in a modal over the whole window (including chrome).

use std::f32::consts::TAU;

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaKind, PosterSize, tmdb_image_url, typograph};
use cinebox_parse::format_bytes;
use iced::advanced::Layout;
use iced::advanced::Renderer as _;
use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget};
use iced::widget::image::Handle as ImageHandle;
use iced::widget::text::Wrapping;
use iced::widget::{Space, button, column, container, mouse_area, row, stack, text};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Fill, FillPortion, Length,
    Rectangle, Shadow, Size, padding,
};

use crate::app::Message;
use crate::ui::home::{ExtraImages, PosterMap};
use crate::ui::scroll::{self, ScrollPane};

use super::state::{Event, FilesPane, ReadyFiles, TorrentFileRow, TorrentState};
use super::{ERR, MUTED, TITLE};

const DIALOG_W: f32 = 720.0;
const COMPACT_H: f32 = 280.0;
const SPINNER: f32 = 44.0;
const STILL_W: f32 = 168.0;
const STILL_H: f32 = 98.0;
const STILL_RADIUS: f32 = 6.0;

pub(super) fn modal<'a>(
    state: &'a TorrentState,
    images: &'a ExtraImages,
    posters: &'a PosterMap,
    poster_size: PosterSize,
    flashing: bool,
    spin_tick: u64,
) -> Element<'a, Message> {
    mouse_area(
        container(
            mouse_area(dialog(
                state,
                images,
                posters,
                poster_size,
                flashing,
                spin_tick,
            ))
            .on_press(Message::Torrents(Event::KeepOpen)),
        )
        .width(Fill)
        .height(Fill)
        .padding(48)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.55).into()),
            ..container::Style::default()
        }),
    )
    .on_press(Message::Torrents(Event::CloseFiles))
    .into()
}

fn dialog<'a>(
    state: &'a TorrentState,
    images: &'a ExtraImages,
    posters: &'a PosterMap,
    poster_size: PosterSize,
    flashing: bool,
    spin_tick: u64,
) -> Element<'a, Message> {
    let compact = matches!(
        state.files,
        FilesPane::Closed | FilesPane::Loading | FilesPane::Failed(_)
    );
    let height = if compact {
        Length::Fixed(COMPACT_H)
    } else {
        Length::Fill
    };

    let mut col = column![text(Msg::TorrentFiles.en()).size(16).color(TITLE)]
        .spacing(12)
        .width(Fill)
        .height(Fill);
    if let Some(status) = status_line(&state.files) {
        col = col.push(text(status).size(13).color(MUTED));
    }
    col = col.push(body(
        state,
        images,
        posters,
        poster_size,
        flashing,
        spin_tick,
    ));

    container(col)
        .padding(16)
        .width(DIALOG_W)
        .height(height)
        .max_height(640)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.11, 0.11, 0.14, 0.98).into()),
            border: iced::border::rounded(12),
            ..container::Style::default()
        })
        .into()
}

fn status_line(pane: &FilesPane) -> Option<&'static str> {
    if matches!(pane, FilesPane::Preloading { .. }) {
        return Some(Msg::Preloading.en());
    }
    None
}

fn body<'a>(
    state: &'a TorrentState,
    images: &'a ExtraImages,
    posters: &'a PosterMap,
    poster_size: PosterSize,
    flashing: bool,
    spin_tick: u64,
) -> Element<'a, Message> {
    match &state.files {
        FilesPane::Closed | FilesPane::Loading => centered_spinner(spin_tick),
        FilesPane::Failed(error) => failed(error),
        FilesPane::Ready(files) => file_list(state, files, images, posters, poster_size, flashing),
        FilesPane::Preloading { files, .. } => stack![
            file_list(state, files, images, posters, poster_size, flashing),
            container(spinner(spin_tick))
                .width(Fill)
                .height(Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(|_| container::Style {
                    background: Some(Color::from_rgba(0.05, 0.05, 0.07, 0.45).into()),
                    ..container::Style::default()
                }),
        ]
        .width(Fill)
        .height(Fill)
        .into(),
    }
}

fn centered_spinner<'a>(spin_tick: u64) -> Element<'a, Message> {
    container(spinner(spin_tick))
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

fn file_list<'a>(
    state: &'a TorrentState,
    files: &'a ReadyFiles,
    images: &'a ExtraImages,
    posters: &'a PosterMap,
    poster_size: PosterSize,
    flashing: bool,
) -> Element<'a, Message> {
    let mut col = column![].spacing(8).width(Fill);
    if files.files.is_empty() {
        col = col.push(text(Msg::NoPlayableFiles.en()).size(14).color(super::LABEL));
        return scroll::vertical_on(ScrollPane::Files, flashing, col);
    }

    let preloading_id = match &state.files {
        FilesPane::Preloading { file_id, .. } => Some(*file_id),
        FilesPane::Closed | FilesPane::Loading | FilesPane::Failed(_) | FilesPane::Ready(_) => None,
    };
    let serial = state.kind == MediaKind::Tv;
    let fallback = posters.get(&(state.kind, state.id)).or_else(|| {
        tmdb_image_url(state.movie.poster_path.as_deref(), poster_size.tmdb_path())
            .and_then(|url| images.get(&url))
    });

    let tagline = state.movie.tagline.as_deref();
    let show_headers = serial && files.files.iter().any(|file| file.season.is_some());
    let mut last_season: Option<Option<u32>> = None;
    for file in &files.files {
        if show_headers {
            let season = file.season;
            let first = last_season.is_none();

            if last_season != Some(season) {
                col = col.push(season_header(season.unwrap_or(1), first));
                last_season = Some(season);
            }
        }

        col = col.push(file_row(
            file,
            files.selected_id,
            preloading_id,
            serial,
            tagline,
            still_handle(file, images, fallback),
        ));
    }

    scroll::vertical_on(ScrollPane::Files, flashing, col)
}

fn still_handle<'a>(
    file: &'a TorrentFileRow,
    images: &'a ExtraImages,
    fallback: Option<&'a ImageHandle>,
) -> Option<&'a ImageHandle> {
    file.still_url
        .as_deref()
        .and_then(|url| images.get(url))
        .or(fallback)
}

fn file_row<'a>(
    file: &'a TorrentFileRow,
    selected_id: Option<i32>,
    preloading_id: Option<i32>,
    serial: bool,
    tagline: Option<&'a str>,
    still: Option<&'a ImageHandle>,
) -> Element<'a, Message> {
    let selected = selected_id == Some(file.id) || preloading_id == Some(file.id);
    let bg = if selected {
        Color::from_rgb(0.21, 0.21, 0.21)
    } else {
        Color::from_rgb(0.11, 0.12, 0.13)
    };

    let inner = row![
        preview(still, file.number),
        column![
            row![
                text(typograph(&file.title))
                    .size(18)
                    .color(TITLE)
                    .wrapping(Wrapping::None)
                    .width(Fill),
                size_pill(format_bytes(file.length)),
            ]
            .spacing(10)
            .align_y(Alignment::Start)
            .width(Fill),
            season_line(file, serial, tagline),
            Space::new().height(Fill),
            progress_track(file.progress()),
        ]
        .spacing(4)
        .padding([8, 0])
        .width(Fill)
        .height(STILL_H),
    ]
    .spacing(12)
    .padding(padding::right(12))
    .align_y(Alignment::Center)
    .width(Fill);

    let card = container(inner)
        .padding(0)
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(bg.into()),
            border: iced::border::rounded(6),
            ..container::Style::default()
        });

    button(card)
        .on_press(Message::Torrents(Event::PickFile(file.id)))
        .padding(0)
        .style(button::text)
        .width(Fill)
        .into()
}

fn preview<'a>(still: Option<&'a ImageHandle>, number: u32) -> Element<'a, Message> {
    let art: Element<'a, Message> = match still {
        Some(handle) => iced::widget::image(handle)
            .width(STILL_W)
            .height(STILL_H)
            .content_fit(ContentFit::Cover)
            .border_radius(STILL_RADIUS)
            .into(),
        None => container(Space::new().width(STILL_W).height(STILL_H))
            .width(STILL_W)
            .height(STILL_H)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.16, 0.16, 0.18).into()),
                border: iced::border::rounded(STILL_RADIUS),
                ..container::Style::default()
            })
            .into(),
    };

    let badge = container(text(number.to_string()).size(14).color(Color::WHITE))
        .padding([3, 8])
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.7).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    stack![
        art,
        container(badge)
            .width(STILL_W)
            .height(STILL_H)
            .padding(4)
            .align_x(Alignment::Start)
            .align_y(Alignment::Start),
    ]
    .width(STILL_W)
    .height(STILL_H)
    .into()
}

fn season_header(season: u32, leading: bool) -> Element<'static, Message> {
    let top = if leading { 0.0 } else { 10.0 };
    let line = container(Space::new().width(Fill).height(1)).style(|_| container::Style {
        background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.14).into()),
        ..container::Style::default()
    });
    container(
        row![
            text(format!("{} {season}", Msg::Season.en()))
                .size(13)
                .color(MUTED),
            line,
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Fill),
    )
    .padding(padding::top(top).right(2.0).bottom(2.0).left(2.0))
    .width(Fill)
    .into()
}

fn season_line<'a>(
    file: &'a TorrentFileRow,
    serial: bool,
    tagline: Option<&'a str>,
) -> Element<'a, Message> {
    if serial {
        let season = file.season.unwrap_or(1);
        let mut line = format!("{} {season}", Msg::Season.en());
        if let Some(episode) = file.episode {
            line = format!("{line}  ·  {} {episode}", Msg::Episode.en());
        }
        if let Some(air_date) = file.air_date.as_deref() {
            line = format!("{line}  ·  {air_date}");
        }
        return text(line).size(13).color(MUTED).into();
    }

    let Some(tagline) = tagline.filter(|s| !s.is_empty()) else {
        return Space::new().height(0).into();
    };

    text(typograph(tagline))
        .size(13)
        .color(MUTED)
        .wrapping(Wrapping::None)
        .into()
}

fn progress_track<'a>(progress: f32) -> Element<'a, Message> {
    let filled = (progress.clamp(0.0, 1.0) * 1000.0).round() as u16;
    let rest = 1000u16.saturating_sub(filled).max(1);
    let fill = container(Space::new().width(Fill).height(3)).style(|_| container::Style {
        background: Some(Color::WHITE.into()),
        border: iced::border::rounded(3),
        ..container::Style::default()
    });
    let empty = container(Space::new().width(Fill).height(3));
    let bar: Element<'a, Message> = if filled == 0 {
        empty.into()
    } else {
        row![
            fill.width(FillPortion(filled)),
            empty.width(FillPortion(rest)),
        ]
        .width(Fill)
        .into()
    };
    container(bar)
        .width(Fill)
        .height(3)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.3).into()),
            border: iced::border::rounded(3),
            ..container::Style::default()
        })
        .into()
}

fn size_pill(label: String) -> Element<'static, Message> {
    container(text(label).size(14).color(TITLE))
        .padding([4, 8])
        .style(|_| container::Style {
            background: Some(Color::from_rgb8(0x26, 0x28, 0x29).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        })
        .into()
}

fn failed(error: &str) -> Element<'static, Message> {
    container(
        column![
            text(error.to_owned()).size(14).color(ERR),
            button(text("Retry")).on_press(Message::Torrents(Event::RetryFiles)),
        ]
        .spacing(12)
        .align_x(Alignment::Center),
    )
    .width(Fill)
    .height(Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .into()
}

fn spinner<'a>(spin_tick: u64) -> Element<'a, Message> {
    Element::new(Spinner {
        angle: spin_tick as f32 * (TAU / 36.0),
    })
}

struct Spinner {
    angle: f32,
}

impl Widget<Message, iced::Theme, iced::Renderer> for Spinner {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(SPINNER), Length::Fixed(SPINNER))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(Size::new(SPINNER, SPINNER))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: iced::mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;
        let radius = 16.0;
        let dot = 5.0;
        const N: i32 = 10;
        for i in 0..N {
            let t = i as f32 / N as f32;
            let a = self.angle + t * TAU;
            let x = cx + a.cos() * radius - dot / 2.0;
            let y = cy + a.sin() * radius - dot / 2.0;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x,
                        y,
                        width: dot,
                        height: dot,
                    },
                    border: Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: (dot / 2.0).into(),
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                Background::Color(Color::from_rgba(0.92, 0.92, 0.96, 0.2 + 0.8 * t)),
            );
        }
    }
}
