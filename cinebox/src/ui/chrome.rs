use cinebox_core::i18n::Msg;
use iced::widget::image::Handle as ImageHandle;
use iced::widget::{Space, button, container, row, text};
use iced::{Color, Element, Fill};

use crate::app::Message;
use crate::nav::Screen;
use crate::ui::backdrop;

pub fn view<'a>(
    screen: Screen,
    body: Element<'a, Message>,
    wallpaper: Option<&'a ImageHandle>,
) -> Element<'a, Message> {
    let nav_button = match screen {
        Screen::Home => button(text(Msg::NavSettings.en())).on_press(Message::OpenSettings),
        Screen::Settings | Screen::Media { .. } | Screen::Person { .. } => {
            button(text(Msg::NavBack.en())).on_press(Message::GoBack)
        }
    };

    let header = container(
        row![
            text(Msg::AppTitle.en()).size(22),
            Space::new().width(Fill),
            nav_button,
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
    )
    .padding(16)
    .width(Fill)
    .style(|_| container::Style {
        background: Some(Color::TRANSPARENT.into()),
        ..container::Style::default()
    });

    let chrome = iced::widget::column![header, body].width(Fill).height(Fill);

    match wallpaper {
        Some(handle) => backdrop::stage(handle, chrome.into()),
        None => chrome.into(),
    }
}
