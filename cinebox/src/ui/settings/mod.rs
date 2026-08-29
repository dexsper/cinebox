//! Settings form: probe state, events, and layout.

mod update;
mod view;

use iced::Task;

pub use update::update;
pub use view::view;

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
    Language(cinebox_core::UiLanguage),
    SystemProxy(bool),
    Loudnorm(bool),
    AutoNext(bool),
    SaveTimecode(bool),
    Scale(cinebox_core::VideoScale),
    Quality(cinebox_core::DefaultQuality),
    ParserKind(cinebox_core::ParserKind),
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
    PosterSize(cinebox_core::PosterSize),
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
