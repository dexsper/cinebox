use std::sync::{Arc, Mutex};

use cinebox_core::i18n::Msg;
use cinebox_player::{
    ClickZone, Engine, FOOTER_LOGICAL, HEADER_LOGICAL, SEEK_SECS, click_zone, format_clock,
};
use egui::{Rect, RichText, Sense, Ui, Vec2};
use egui_material_icons::icons::{ICON_PAUSE, ICON_PLAY_ARROW, ICON_SKIP_NEXT};

use crate::nav::NavAction;
use crate::screens::play::PlayRequest;
use crate::screens::torrents::TorrentFileRow;
use crate::services::Services;
use crate::theme::Theme;

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

struct PlayerView {
    title: String,
    error: Option<String>,
    time: f64,
    duration: f64,
    has_next: bool,
    paused: bool,
    aid: i64,
    sid: i64,
}

impl PlayerState {
    fn from_request(req: PlayRequest) -> Self {
        Self {
            title: req.title,
            hash: req.hash,
            files: req.files,
            file_index: req.file_index,
            paused: false,
            time: req.start,
            duration: 0.0,
            error: None,
            aid: 0,
            sid: 0,
            play_url: req.url,
        }
    }

    #[must_use]
    pub fn has_next(&self) -> bool {
        self.file_index + 1 < self.files.len()
    }
}

#[derive(Default)]
pub struct PlayerScreen {
    state: Option<PlayerState>,
}

impl PlayerScreen {
    pub fn start(&mut self, req: PlayRequest, svc: &mut Services) {
        self.state = Some(PlayerState::from_request(req));
        self.load(svc);
    }

    pub fn stop(&mut self, svc: &Services) {
        if let Some(engine) = &svc.engine
            && let Ok(engine) = engine.lock()
        {
            engine.stop();
        }
        self.state = None;
    }

    pub fn tick(&mut self, svc: &mut Services) {
        let Some(engine) = &svc.engine else {
            return;
        };
        let Ok(engine) = engine.lock() else {
            return;
        };
        let snap = engine.snapshot();
        let auto = svc.settings.player.auto_next;
        let go_next = {
            let Some(state) = &mut self.state else {
                return;
            };
            state.paused = snap.paused;
            state.time = snap.time;
            state.duration = snap.duration;
            state.aid = snap.aid;
            state.sid = snap.sid;
            snap.eof && snap.duration > 1.0 && auto && state.has_next()
        };
        drop(engine);
        if go_next {
            self.next_file(svc);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        self.handle_keys(ui, svc);
        let Some(view) = self.state.as_ref().map(|s| PlayerView {
            title: s.title.clone(),
            error: s.error.clone(),
            time: s.time,
            duration: s.duration,
            has_next: s.has_next(),
            paused: s.paused,
            aid: s.aid,
            sid: s.sid,
        }) else {
            ui.label(RichText::new(Msg::LoadingCard.t()).color(theme.muted));
            return None;
        };

        let mut seek = None;
        let mut toggle = false;
        let mut next = false;
        let mut cycle_audio = false;
        let mut cycle_subs = false;

        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), HEADER_LOGICAL),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, theme.panel);
                ui.add_space(12.0);
                ui.label(
                    RichText::new(&view.title)
                        .font(theme.title_font(theme.text_subtitle))
                        .color(theme.title),
                );
            },
        );

        let video = ui.available_size() - Vec2::new(0.0, FOOTER_LOGICAL);
        let (rect, response) = ui.allocate_exact_size(video, Sense::click());
        if let Some(error) = view.error.as_deref() {
            ui.painter().rect_filled(rect, 0.0, theme.video_bg);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                error,
                theme.ui_font(theme.text_body),
                theme.err,
            );
        } else {
            paint_video(ui, rect, svc.engine.clone(), theme);
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let x_ratio = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                match click_zone(x_ratio) {
                    ClickZone::SeekBack => seek = Some(-SEEK_SECS),
                    ClickZone::Pause => toggle = true,
                    ClickZone::SeekFwd => seek = Some(SEEK_SECS),
                }
            }
        }

        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), FOOTER_LOGICAL),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, theme.panel);
                let clock = format!(
                    "{} / {}",
                    format_clock(view.time),
                    format_clock(view.duration)
                );
                ui.label(RichText::new(clock).size(theme.text_body).color(theme.muted_bright));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if view.has_next
                        && ui
                            .add(egui::Button::new(ICON_SKIP_NEXT.rich_text().size(theme.text_icon_lg)))
                            .clicked()
                    {
                        next = true;
                    }
                    let subs = if view.sid > 0 {
                        format!("{} {}", Msg::Subtitles.t(), view.sid)
                    } else {
                        Msg::Subtitles.t().to_owned()
                    };
                    if ui.button(subs).clicked() {
                        cycle_subs = true;
                    }
                    let audio = if view.aid > 0 {
                        format!("{} {}", Msg::Audio.t(), view.aid)
                    } else {
                        Msg::Audio.t().to_owned()
                    };
                    if ui.button(audio).clicked() {
                        cycle_audio = true;
                    }
                    let icon = if view.paused {
                        ICON_PLAY_ARROW
                    } else {
                        ICON_PAUSE
                    };
                    if ui
                        .add(egui::Button::new(icon.rich_text().size(theme.text_icon_lg)))
                        .on_hover_text(if view.paused {
                            Msg::Play.t()
                        } else {
                            Msg::Pause.t()
                        })
                        .clicked()
                    {
                        toggle = true;
                    }
                });
            },
        );

        if let Some(delta) = seek {
            self.seek(svc, delta);
        }
        if toggle {
            self.toggle(svc);
        }
        if next {
            self.next_file(svc);
        }
        if cycle_audio
            && let Some(engine) = &svc.engine
            && let Ok(engine) = engine.lock()
        {
            let _ = engine.cycle_audio();
        }
        if cycle_subs
            && let Some(engine) = &svc.engine
            && let Ok(engine) = engine.lock()
        {
            let _ = engine.cycle_subs();
        }

        None
    }

    fn handle_keys(&mut self, ui: &Ui, svc: &mut Services) {
        if ui.input(|i| i.key_pressed(egui::Key::Space)) {
            self.toggle(svc);
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.seek(svc, -SEEK_SECS);
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.seek(svc, SEEK_SECS);
        }
    }

    fn load(&mut self, svc: &mut Services) {
        let Some(state) = &self.state else {
            return;
        };
        let Some(engine) = &svc.engine else {
            if let Some(state) = &mut self.state {
                state.error = Some(Msg::MpvRenderFailed.t().to_owned());
            }
            return;
        };
        let header = cinebox_torrserver::mpv_http_header_fields(
            &svc.settings.torrserver.username,
            svc.settings.torrserver.password.expose(),
        );
        let opts = cinebox_player::PlayOpts {
            http_header_fields: header.as_deref(),
            loudnorm: svc.settings.player.loudnorm,
            scale: svc.settings.player.scale,
            start_seconds: state.time,
        };
        let url = state.play_url.clone();
        let Ok(engine) = engine.lock() else {
            return;
        };
        if let Err(error) = engine.load(&url, opts)
            && let Some(state) = &mut self.state
        {
            state.error = Some(error.to_string());
        }
    }

    fn toggle(&mut self, svc: &Services) {
        let Some(engine) = &svc.engine else {
            return;
        };
        let Ok(engine) = engine.lock() else {
            return;
        };
        if let Ok(paused) = engine.toggle_pause()
            && let Some(state) = &mut self.state
        {
            state.paused = paused;
        }
    }

    fn seek(&mut self, svc: &Services, delta: f64) {
        let Some(engine) = &svc.engine else {
            return;
        };
        if let Ok(engine) = engine.lock() {
            let _ = engine.seek(delta);
        }
    }

    fn next_file(&mut self, svc: &mut Services) {
        let next = {
            let Some(state) = &mut self.state else {
                return;
            };
            if !state.has_next() {
                return;
            }
            state.file_index += 1;
            let Some(file) = state.files.get(state.file_index).cloned() else {
                return;
            };
            state.title = file.title.clone();
            state.time = file.timecode;
            let url = match cinebox_torrserver::stream_url(
                &svc.settings.torrserver.url,
                &file.path,
                &state.hash,
                file.id,
                cinebox_torrserver::StreamFlag::Play,
            ) {
                Ok(url) => url,
                Err(error) => {
                    state.error = Some(error.to_string());
                    return;
                }
            };
            state.play_url = url.clone();
            (url, file.timecode)
        };
        if let Some(state) = &mut self.state {
            state.time = next.1;
        }
        self.load(svc);
    }
}

fn paint_video(ui: &mut Ui, rect: Rect, engine: Option<Arc<Mutex<Engine>>>, theme: &Theme) {
    ui.painter().rect_filled(rect, 0.0, theme.video_bg);
    let Some(engine) = engine else {
        return;
    };
    ui.painter().add(egui::PaintCallback {
        rect,
        callback: std::sync::Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
            let vp = info.viewport_in_pixels();
            let fbo = painter.intermediate_fbo().map(|fb| fb.0.get()).unwrap_or(0);
            if let Ok(engine) = engine.lock() {
                let _ = engine.render(fbo, vp.width_px, vp.height_px);
            }
        })),
    });
}
