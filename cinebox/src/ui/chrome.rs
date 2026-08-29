use cinebox_core::i18n::Msg;
use iced::widget::{Space, button, container, row, text};
use iced::{Element, Fill};

use crate::app::Message;
use crate::nav::Screen;

pub fn view<'a>(screen: Screen, body: Element<'a, Message>) -> Element<'a, Message> {
    let nav_button = match screen {
        Screen::Home => button(text(Msg::NavSettings.en())).on_press(Message::OpenSettings),
        Screen::Settings => button(text(Msg::NavBack.en())).on_press(Message::GoBack),
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
    .width(Fill);

    iced::widget::column![header, body]
        .width(Fill)
        .height(Fill)
        .into()
}
