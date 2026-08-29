//! In-window player chrome around the mpv HWND hole.

use cinebox_core::i18n::Msg;
use cinebox_core::typograph;
use cinebox_player::{FOOTER_LOGICAL, HEADER_LOGICAL, format_clock};
use iced::widget::{Space, button, column, container, mouse_area, row, text};
use iced::{Alignment, Color, Element, Fill, FillPortion, Length};

use crate::app::Message;
use crate::ui::torrents::TorrentFileRow;

const MUTED: Color = Color::from_rgb(0.78, 0.78, 0.82);
const ERR: Color = Color::from_rgb(0.92, 0.38, 0.38);
const TITLE: Color = Color::from_rgb(0.96, 0.96, 0.97);

#[derive(Debug, Clone, Copy)]
pub enum Event {
    TogglePause,
    SeekBack,
    SeekFwd,
    CycleAudio,
    CycleSubs,
    Next,
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub title: String,
    pub hash: String,
    pub files: Vec<TorrentFileRow>,
    pub file_index: usize,
    pub paused: bool,
    pub time: f64,
    pub duration: f64,
    pub error: Option<String>,
    pub aid: i64,
    pub sid: i64,
    pub play_url: String,
}

impl PlayerState {
    #[must_use]
    pub fn has_next(&self) -> bool {
        self.file_index + 1 < self.files.len()
    }
}

pub fn view(state: &PlayerState) -> Element<'_, Message> {
    let title = typograph(&state.title);
    let head = container(
        row![
            button(text(Msg::NavBack.en())).on_press(Message::GoBack),
            text(title).size(18).color(TITLE),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([12, 16])
    .width(Fill)
    .height(Length::Fixed(HEADER_LOGICAL))
    .style(|_| container::Style {
        background: Some(Color::from_rgb(0.08, 0.08, 0.1).into()),
        ..container::Style::default()
    });

    let hole: Element<'_, Message> = if let Some(error) = state.error.as_deref() {
        container(text(error.to_owned()).size(14).color(ERR))
            .width(Fill)
            .height(Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|_| container::Style {
                background: Some(Color::BLACK.into()),
                ..container::Style::default()
            })
            .into()
    } else {
        container(
            row![
                zone(Event::SeekBack),
                zone(Event::TogglePause),
                zone(Event::SeekFwd),
            ]
            .width(Fill)
            .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(|_| container::Style {
            background: Some(Color::BLACK.into()),
            ..container::Style::default()
        })
        .into()
    };

    let clock = format!(
        "{} / {}",
        format_clock(state.time),
        format_clock(state.duration)
    );
    let play_label = if state.paused {
        Msg::Play.en()
    } else {
        Msg::Pause.en()
    };
    let audio = if state.aid > 0 {
        format!("{} {}", Msg::Audio.en(), state.aid)
    } else {
        Msg::Audio.en().to_owned()
    };
    let subs = if state.sid > 0 {
        format!("{} {}", Msg::Subtitles.en(), state.sid)
    } else {
        Msg::Subtitles.en().to_owned()
    };

    let mut controls = row![
        text(clock).size(14).color(MUTED),
        Space::new().width(Fill),
        button(text(play_label)).on_press(Message::Player(Event::TogglePause)),
        button(text(audio)).on_press(Message::Player(Event::CycleAudio)),
        button(text(subs)).on_press(Message::Player(Event::CycleSubs)),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .width(Fill);

    if state.has_next() {
        controls =
            controls.push(button(text(Msg::NextFile.en())).on_press(Message::Player(Event::Next)));
    }

    let foot = container(controls)
        .padding([12, 16])
        .width(Fill)
        .height(Length::Fixed(FOOTER_LOGICAL))
        .style(|_| container::Style {
            background: Some(Color::from_rgb(0.08, 0.08, 0.1).into()),
            ..container::Style::default()
        });

    column![head, hole, foot].width(Fill).height(Fill).into()
}

fn zone<'a>(event: Event) -> Element<'a, Message> {
    container(mouse_area(Space::new().width(Fill).height(Fill)).on_press(Message::Player(event)))
        .width(FillPortion(1))
        .height(Fill)
        .into()
}
