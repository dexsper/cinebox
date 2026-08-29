use std::path::Path;

use cinebox_core::{
    DefaultQuality, ParserKind, PosterSize, SecretString, Settings, UiLanguage, VideoScale,
};
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input};
use iced::{Color, Element, Fill, Task};

use crate::app::Message as AppMessage;
use crate::ui::scroll;

const OK_COLOR: Color = Color::from_rgb(0.45, 0.82, 0.55);
const ERR_COLOR: Color = Color::from_rgb(0.92, 0.38, 0.38);
const MUTED: Color = Color::from_rgb(0.65, 0.65, 0.68);

/// Connectivity / speed-test status for one action.
#[derive(Debug, Clone, Default)]
pub enum Probe {
    #[default]
    Idle,
    Running,
    Ok(String),
    Err(String),
}

/// All settings probes.
#[derive(Debug, Clone, Default)]
pub struct Probes {
    pub torrserver: Probe,
    pub parser: Probe,
    pub tmdb: Probe,
    pub speed: Probe,
}

/// Settings form events. Secret field values travel here (Iced requirement); do not log them.
#[derive(Debug, Clone)]
pub enum Message {
    Language(UiLanguage),
    SystemProxy(bool),
    Loudnorm(bool),
    AutoNext(bool),
    SaveTimecode(bool),
    Scale(VideoScale),
    Quality(DefaultQuality),
    ParserKind(ParserKind),
    ParserUrl(String),
    ParserKey(String),
    TorrUrl(String),
    SaveToDb(bool),
    WaitPreload(bool),
    TrackTimecode(bool),
    TorrUser(String),
    TorrPass(String),
    TmdbKey(String),
    TmdbLang(String),
    PosterSize(PosterSize),
    SpeedMb(u32),
    PingTorrServer,
    PingParser,
    PingTmdb,
    RunSpeedTest,
    TorrEcho(Result<String, String>),
    ParserPinged(Result<String, String>),
    TmdbPinged(Result<String, String>),
    SpeedDone(Result<String, String>),
}

pub struct Update {
    pub persist: bool,
    pub task: Task<Message>,
}

pub fn update(
    settings: &mut Settings,
    probes: &mut Probes,
    speed_mb: &mut u32,
    message: Message,
) -> Update {
    match message {
        Message::Language(value) => {
            settings.interface.language = value;
            persist_only()
        }
        Message::SystemProxy(value) => {
            settings.interface.use_system_proxy = value;
            probes.tmdb = Probe::Idle;
            probes.parser = Probe::Idle;
            persist_only()
        }
        Message::Loudnorm(value) => {
            settings.player.loudnorm = value;
            persist_only()
        }
        Message::AutoNext(value) => {
            settings.player.auto_next = value;
            persist_only()
        }
        Message::SaveTimecode(value) => {
            settings.player.save_timecode = value;
            persist_only()
        }
        Message::Scale(value) => {
            settings.player.scale = value;
            persist_only()
        }
        Message::Quality(value) => {
            settings.player.default_quality = value;
            persist_only()
        }
        Message::ParserKind(value) => {
            settings.parser.kind = value;
            probes.parser = Probe::Idle;
            persist_only()
        }
        Message::ParserUrl(value) => {
            settings.parser.url = value;
            probes.parser = Probe::Idle;
            persist_only()
        }
        Message::ParserKey(value) => {
            settings.parser.api_key = SecretString::from(value);
            probes.parser = Probe::Idle;
            persist_only()
        }
        Message::TorrUrl(value) => {
            settings.torrserver.url = value;
            probes.torrserver = Probe::Idle;
            probes.speed = Probe::Idle;
            persist_only()
        }
        Message::SaveToDb(value) => {
            settings.torrserver.save_to_db = value;
            persist_only()
        }
        Message::WaitPreload(value) => {
            settings.torrserver.wait_preload = value;
            persist_only()
        }
        Message::TrackTimecode(value) => {
            settings.torrserver.track_timecode = value;
            persist_only()
        }
        Message::TorrUser(value) => {
            settings.torrserver.username = value;
            probes.torrserver = Probe::Idle;
            probes.speed = Probe::Idle;
            persist_only()
        }
        Message::TorrPass(value) => {
            settings.torrserver.password = SecretString::from(value);
            probes.torrserver = Probe::Idle;
            probes.speed = Probe::Idle;
            persist_only()
        }
        Message::TmdbKey(value) => {
            settings.tmdb.api_key = SecretString::from(value.trim());
            probes.tmdb = Probe::Idle;
            persist_only()
        }
        Message::TmdbLang(value) => {
            let trimmed = value.trim();
            settings.tmdb.data_language = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            };
            persist_only()
        }
        Message::PosterSize(value) => {
            settings.tmdb.poster_size = value;
            persist_only()
        }
        Message::SpeedMb(value) => {
            *speed_mb = value;
            probes.speed = Probe::Idle;
            Update {
                persist: false,
                task: Task::none(),
            }
        }
        Message::PingTorrServer => {
            probes.torrserver = Probe::Running;
            let url = settings.torrserver.url.clone();
            let user = settings.torrserver.username.clone();
            let pass = settings.torrserver.password.expose().to_owned();
            Update {
                persist: false,
                task: Task::perform(
                    async move {
                        cinebox_torrserver::echo(&url, &user, &pass)
                            .await
                            .map_err(|error| error.to_string())
                    },
                    Message::TorrEcho,
                ),
            }
        }
        Message::PingParser => {
            probes.parser = Probe::Running;
            let kind = settings.parser.kind;
            let url = settings.parser.url.clone();
            let key = settings.parser.api_key.expose().to_owned();
            let use_system_proxy = settings.interface.use_system_proxy;
            Update {
                persist: false,
                task: Task::perform(
                    async move {
                        cinebox_indexer::ping(kind, &url, &key, use_system_proxy)
                            .await
                            .map_err(|error| error.to_string())
                    },
                    Message::ParserPinged,
                ),
            }
        }
        Message::PingTmdb => {
            probes.tmdb = Probe::Running;
            let key = settings.tmdb.api_key.expose().to_owned();
            let use_system_proxy = settings.interface.use_system_proxy;
            Update {
                persist: false,
                task: Task::perform(
                    async move {
                        cinebox_tmdb::check_api_key(&key, use_system_proxy)
                            .await
                            .map_err(|error| error.to_string())
                    },
                    Message::TmdbPinged,
                ),
            }
        }
        Message::RunSpeedTest => {
            probes.speed = Probe::Running;
            let url = settings.torrserver.url.clone();
            let user = settings.torrserver.username.clone();
            let pass = settings.torrserver.password.expose().to_owned();
            let size_mb = *speed_mb;
            Update {
                persist: false,
                task: Task::perform(
                    async move {
                        cinebox_torrserver::speed_test(&url, &user, &pass, size_mb)
                            .await
                            .map(|report| report.summary())
                            .map_err(|error| error.to_string())
                    },
                    Message::SpeedDone,
                ),
            }
        }
        Message::TorrEcho(result) => {
            probes.torrserver = into_probe(result, |version| format!("connected ({version})"));
            no_persist()
        }
        Message::ParserPinged(result) => {
            probes.parser = into_probe(result, |msg| msg);
            no_persist()
        }
        Message::TmdbPinged(result) => {
            probes.tmdb = into_probe(result, |msg| msg);
            no_persist()
        }
        Message::SpeedDone(result) => {
            probes.speed = into_probe(result, |msg| msg);
            no_persist()
        }
    }
}

fn persist_only() -> Update {
    Update {
        persist: true,
        task: Task::none(),
    }
}

fn no_persist() -> Update {
    Update {
        persist: false,
        task: Task::none(),
    }
}

fn into_probe(result: Result<String, String>, ok: impl FnOnce(String) -> String) -> Probe {
    match result {
        Ok(value) => Probe::Ok(ok(value)),
        Err(error) => Probe::Err(error),
    }
}

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
        Probe::Ok(msg) => text(msg.clone()).size(13).color(OK_COLOR).into(),
        Probe::Err(msg) => text(msg.clone()).size(13).color(ERR_COLOR).into(),
    }
}
