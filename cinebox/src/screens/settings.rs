use cinebox_core::i18n::Msg;
use cinebox_core::{DefaultQuality, ParserKind, PosterSize, SecretString, UiLanguage, VideoScale};
use egui::{ComboBox, RichText, TextEdit, Ui};
use egui_async::Bind;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::scroll;

pub struct SettingsScreen {
    speed_mb: u32,
    torr: Bind<String, String>,
    parser: Bind<String, String>,
    tmdb: Bind<String, String>,
    speed: Bind<String, String>,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self {
            speed_mb: cinebox_torrserver::SPEED_TEST_SIZES_MB[0],
            torr: Bind::new(true),
            parser: Bind::new(true),
            tmdb: Bind::new(true),
            speed: Bind::new(true),
        }
    }
}

impl SettingsScreen {
    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        let mut persist = false;
        scroll::vertical(ui, "settings-page", |ui| {
            ui.label(
                RichText::new(Msg::SettingsTitle.en())
                    .size(22.0)
                    .color(theme.title),
            );
            let path = svc
                .store
                .as_ref()
                .map(|s| s.path().display().to_string())
                .unwrap_or_else(|| String::from("(in-memory only)"));
            ui.label(
                RichText::new(format!("{}: {path}", Msg::SettingsPath.en()))
                    .size(13.0)
                    .color(theme.muted),
            );
            if let Some(error) = &svc.load_error {
                ui.label(RichText::new(Msg::SettingsLoadError.en()).color(theme.err));
                ui.label(RichText::new(error).size(13.0).color(theme.err));
            }
            if let Some(error) = &svc.save_error {
                ui.label(RichText::new(format!("Could not save: {error}")).color(theme.err));
            }

            ui.add_space(12.0);
            ui.label(RichText::new("Interface").size(18.0).color(theme.title));
            persist |= combo(
                ui,
                "language",
                "Language",
                &mut svc.settings.interface.language,
                UiLanguage::ALL,
            );
            persist |= ui
                .checkbox(
                    &mut svc.settings.interface.use_system_proxy,
                    "Use system proxy",
                )
                .changed();
            ui.label(
                RichText::new("Applies to TMDB and parser. TorrServer always connects directly.")
                    .size(12.0)
                    .color(theme.muted),
            );

            ui.add_space(12.0);
            ui.label(RichText::new("Player").size(18.0).color(theme.title));
            persist |= ui
                .checkbox(&mut svc.settings.player.loudnorm, "Loudnorm")
                .changed();
            persist |= ui
                .checkbox(
                    &mut svc.settings.player.auto_next,
                    "Play next file automatically",
                )
                .changed();
            persist |= ui
                .checkbox(&mut svc.settings.player.save_timecode, "Save timecode")
                .changed();
            persist |= combo(
                ui,
                "scale",
                "Scale",
                &mut svc.settings.player.scale,
                VideoScale::ALL,
            );
            persist |= combo(
                ui,
                "quality",
                "Default quality",
                &mut svc.settings.player.default_quality,
                DefaultQuality::ALL,
            );

            ui.add_space(12.0);
            ui.label(RichText::new("Parser").size(18.0).color(theme.title));
            persist |= combo(
                ui,
                "parser-kind",
                "Type",
                &mut svc.settings.parser.kind,
                ParserKind::ALL,
            );
            persist |= labeled_text(ui, "URL", &mut svc.settings.parser.url);
            persist |= secret_edit(ui, "API key", &mut svc.settings.parser.api_key);
            probe_row(ui, theme, "Test parser", &mut self.parser, || {
                jobs::ping_parser(svc.settings.clone())
            });

            ui.add_space(12.0);
            ui.label(RichText::new("TorrServer").size(18.0).color(theme.title));
            persist |= labeled_text(ui, "URL", &mut svc.settings.torrserver.url);
            persist |= ui
                .checkbox(
                    &mut svc.settings.torrserver.save_to_db,
                    "Save torrents to server DB",
                )
                .changed();
            persist |= ui
                .checkbox(
                    &mut svc.settings.torrserver.wait_preload,
                    "Wait for preload",
                )
                .changed();
            persist |= ui
                .checkbox(
                    &mut svc.settings.torrserver.track_timecode,
                    "Track timecode on server",
                )
                .changed();
            persist |= labeled_text(ui, "Username", &mut svc.settings.torrserver.username);
            persist |= secret_edit(ui, "Password", &mut svc.settings.torrserver.password);
            probe_row(ui, theme, "Ping", &mut self.torr, || {
                jobs::ping_torrserver(svc.settings.clone())
            });
            ui.horizontal(|ui| {
                ui.label("Speed test size (MB)");
                combo(
                    ui,
                    "speed-mb",
                    "",
                    &mut self.speed_mb,
                    &cinebox_torrserver::SPEED_TEST_SIZES_MB,
                );
                let size = self.speed_mb;
                let settings = svc.settings.clone();
                if ui.button("Run").clicked() {
                    self.speed.clear();
                    self.speed.request(jobs::speed_test(settings, size));
                }
            });
            show_probe(ui, &mut self.speed, theme);

            ui.add_space(12.0);
            ui.label(RichText::new("TMDB").size(18.0).color(theme.title));
            persist |= secret_edit(ui, "API key", &mut svc.settings.tmdb.api_key);
            ui.label(
                RichText::new(
                    "Short «API key» from themoviedb.org (32 hex). Not the JWT access token.",
                )
                .size(12.0)
                .color(theme.muted),
            );
            let mut lang = svc.settings.tmdb.data_language.clone().unwrap_or_default();
            ui.label(
                RichText::new("Data language (empty = OS later)")
                    .size(13.0)
                    .color(theme.muted),
            );
            if ui
                .add(TextEdit::singleline(&mut lang).hint_text("en-US"))
                .changed()
            {
                let trimmed = lang.trim();
                svc.settings.tmdb.data_language = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                };
                persist = true;
            }
            persist |= combo(
                ui,
                "poster-size",
                "Poster size",
                &mut svc.settings.tmdb.poster_size,
                PosterSize::ALL,
            );
            probe_row(ui, theme, "Check API key", &mut self.tmdb, || {
                jobs::ping_tmdb(svc.settings.clone(), svc.db.clone())
            });
            if ui.button(Msg::ClearCache.en()).clicked() {
                svc.clear_tmdb_cache();
            }
        });

        if persist {
            svc.persist();
        }
        None
    }
}

fn combo<T: Copy + PartialEq + std::fmt::Display>(
    ui: &mut Ui,
    id: &str,
    label: &str,
    value: &mut T,
    options: &[T],
) -> bool {
    let mut changed = false;
    if !label.is_empty() {
        ui.label(RichText::new(label).size(13.0));
    }
    ComboBox::from_id_salt(id)
        .selected_text(value.to_string())
        .show_ui(ui, |ui| {
            for opt in options {
                changed |= ui.selectable_value(value, *opt, opt.to_string()).changed();
            }
        });
    changed
}

fn labeled_text(ui: &mut Ui, label: &str, value: &mut String) -> bool {
    ui.label(RichText::new(label).size(13.0));
    ui.add(TextEdit::singleline(value).desired_width(f32::INFINITY))
        .changed()
}

fn secret_edit(ui: &mut Ui, label: &str, secret: &mut SecretString) -> bool {
    ui.label(RichText::new(label).size(13.0));
    let mut value = secret.expose().to_owned();
    let changed = ui
        .add(
            TextEdit::singleline(&mut value)
                .password(true)
                .desired_width(f32::INFINITY),
        )
        .changed();
    if changed {
        *secret = SecretString::from(value);
    }
    changed
}

fn probe_row<F, Fut>(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    bind: &mut Bind<String, String>,
    start: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    if ui.button(label).clicked() {
        bind.clear();
        bind.request(start());
    }
    show_probe(ui, bind, theme);
}

fn show_probe(ui: &mut Ui, bind: &mut Bind<String, String>, theme: &Theme) {
    match bind.read() {
        None => {}
        Some(Ok(msg)) => {
            ui.label(RichText::new(msg).size(13.0).color(theme.ok));
        }
        Some(Err(msg)) => {
            ui.label(RichText::new(msg).size(13.0).color(theme.err));
        }
    }
}
