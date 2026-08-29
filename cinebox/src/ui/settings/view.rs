//! Settings form layout.

use std::path::Path;

use cinebox_core::{DefaultQuality, ParserKind, PosterSize, Settings, UiLanguage, VideoScale};
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input};
use iced::{Color, Element, Fill};

use crate::app::Message as AppMessage;
use crate::ui::scroll;

use super::{Message, Probe, Probes};

const OK_COLOR: Color = Color::from_rgb(0.45, 0.82, 0.55);
const ERR_COLOR: Color = Color::from_rgb(0.92, 0.38, 0.38);
const MUTED: Color = Color::from_rgb(0.65, 0.65, 0.68);

pub fn view<'a>(
    path: Option<&'a Path>,
    load_error: Option<&'a str>,
    save_error: Option<&'a str>,
    settings: &'a Settings,
    probes: &'a Probes,
    speed_mb: u32,
    page_flashing: bool,
) -> Element<'a, AppMessage> {
    container(scroll::vertical(
        page_flashing,
        inner(path, load_error, save_error, settings, probes, speed_mb).map(AppMessage::Settings),
    ))
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn inner<'a>(
    path: Option<&'a Path>,
    load_error: Option<&'a str>,
    save_error: Option<&'a str>,
    settings: &'a Settings,
    probes: &'a Probes,
    speed_mb: u32,
) -> Element<'a, Message> {
    let path_label = path.map_or_else(
        || String::from("(in-memory only)"),
        |p| p.display().to_string(),
    );

    let mut form = column![
        text("Settings").size(22),
        text(format!("Config file: {path_label}"))
            .size(13)
            .color(MUTED),
    ]
    .spacing(12);

    if let Some(error) = load_error {
        form = form.push(text("Could not load settings; using defaults.").color(ERR_COLOR));
        form = form.push(text(error.to_owned()).size(13).color(ERR_COLOR));
    }

    if let Some(error) = save_error {
        form = form.push(text(format!("Could not save: {error}")).color(ERR_COLOR));
    }

    form = form
        .push(section("Interface"))
        .push(labeled(
            "Language",
            pick_list(
                UiLanguage::ALL,
                Some(settings.interface.language),
                Message::Language,
            )
            .into(),
        ))
        .push(
            checkbox(settings.interface.use_system_proxy)
                .label("Use system proxy")
                .on_toggle(Message::SystemProxy),
        )
        .push(
            text("Applies to TMDB and parser. TorrServer always connects directly.")
                .size(12)
                .color(MUTED),
        )
        .push(section("Player"))
        .push(
            checkbox(settings.player.loudnorm)
                .label("Loudnorm")
                .on_toggle(Message::Loudnorm),
        )
        .push(
            checkbox(settings.player.auto_next)
                .label("Play next file automatically")
                .on_toggle(Message::AutoNext),
        )
        .push(
            checkbox(settings.player.save_timecode)
                .label("Save timecode")
                .on_toggle(Message::SaveTimecode),
        )
        .push(labeled(
            "Scale",
            pick_list(VideoScale::ALL, Some(settings.player.scale), Message::Scale).into(),
        ))
        .push(labeled(
            "Default quality",
            pick_list(
                DefaultQuality::ALL,
                Some(settings.player.default_quality),
                Message::Quality,
            )
            .into(),
        ))
        .push(section("Parser"))
        .push(labeled(
            "Type",
            pick_list(
                ParserKind::ALL,
                Some(settings.parser.kind),
                Message::ParserKind,
            )
            .into(),
        ))
        .push(labeled(
            "URL",
            text_input("http://127.0.0.1:9117", &settings.parser.url)
                .on_input(Message::ParserUrl)
                .id("settings-parser-url")
                .padding(8)
                .into(),
        ))
        .push(labeled(
            "API key",
            text_input("", settings.parser.api_key.expose())
                .secure(true)
                .on_input(Message::ParserKey)
                .id("settings-parser-key")
                .padding(8)
                .into(),
        ))
        .push(probe_row(
            "Test parser",
            Message::PingParser,
            &probes.parser,
        ))
        .push(section("TorrServer"))
        .push(labeled(
            "URL",
            text_input("http://127.0.0.1:8090", &settings.torrserver.url)
                .on_input(Message::TorrUrl)
                .id("settings-ts-url")
                .padding(8)
                .into(),
        ))
        .push(
            checkbox(settings.torrserver.save_to_db)
                .label("Save torrents to server DB")
                .on_toggle(Message::SaveToDb),
        )
        .push(
            checkbox(settings.torrserver.wait_preload)
                .label("Wait for preload")
                .on_toggle(Message::WaitPreload),
        )
        .push(
            checkbox(settings.torrserver.track_timecode)
                .label("Track timecode on server")
                .on_toggle(Message::TrackTimecode),
        )
        .push(labeled(
            "Username",
            text_input("optional", &settings.torrserver.username)
                .on_input(Message::TorrUser)
                .id("settings-ts-user")
                .padding(8)
                .into(),
        ))
        .push(labeled(
            "Password",
            text_input("", settings.torrserver.password.expose())
                .secure(true)
                .on_input(Message::TorrPass)
                .id("settings-ts-pass")
                .padding(8)
                .into(),
        ))
        .push(probe_row(
            "Ping",
            Message::PingTorrServer,
            &probes.torrserver,
        ))
        .push(
            row![
                text("Speed test size (MB)"),
                pick_list(
                    cinebox_torrserver::SPEED_TEST_SIZES_MB,
                    Some(speed_mb),
                    Message::SpeedMb,
                ),
                button(text("Run")).on_press(Message::RunSpeedTest),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        )
        .push(probe_status(&probes.speed))
        .push(section("TMDB"))
        .push(labeled(
            "API key",
            text_input("", settings.tmdb.api_key.expose())
                .secure(true)
                .on_input(Message::TmdbKey)
                .id("settings-tmdb-key")
                .padding(8)
                .into(),
        ))
        .push(
            text("Short «API key» from themoviedb.org (32 hex). Not the JWT access token.")
                .size(12)
                .color(MUTED),
        )
        .push(labeled(
            "Data language (empty = OS later)",
            text_input(
                "en-US",
                settings.tmdb.data_language.as_deref().unwrap_or(""),
            )
            .on_input(Message::TmdbLang)
            .id("settings-tmdb-lang")
            .padding(8)
            .into(),
        ))
        .push(labeled(
            "Poster size",
            pick_list(
                PosterSize::ALL,
                Some(settings.tmdb.poster_size),
                Message::PosterSize,
            )
            .into(),
        ))
        .push(probe_row("Check API key", Message::PingTmdb, &probes.tmdb));

    form.padding([0, 8]).width(Fill).into()
}

fn section<'a>(title: &'static str) -> Element<'a, Message> {
    text(title).size(18).into()
}

fn labeled<'a>(label: &'static str, field: Element<'a, Message>) -> Element<'a, Message> {
    column![text(label).size(13).color(MUTED), field]
        .spacing(4)
        .into()
}

fn probe_row<'a>(label: &'static str, on_press: Message, probe: &'a Probe) -> Element<'a, Message> {
    column![button(text(label)).on_press(on_press), probe_status(probe),]
        .spacing(6)
        .into()
}

fn probe_status<'a>(probe: &'a Probe) -> Element<'a, Message> {
    match probe {
        Probe::Idle => text(" ").size(13).into(),
        Probe::Running => text("Checking…").size(13).color(MUTED).into(),
        Probe::Ok(msg) => text(msg.as_str()).size(13).color(OK_COLOR).into(),
        Probe::Err(msg) => text(msg.as_str()).size(13).color(ERR_COLOR).into(),
    }
}
