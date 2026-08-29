//! Settings persist + probe orchestration.

use cinebox_core::{SecretString, Settings};
use iced::Task;

use super::{Message, Probe, Probes, Update};

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
