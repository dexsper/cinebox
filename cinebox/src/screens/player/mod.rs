//! In-window player: full-bleed video, floating auto-hiding chrome, popups,
//! true OS fullscreen, and a TorrServer buffering phase.

mod buffering;
mod overlay;
mod playlist_popup;
mod settings_popup;
mod volume;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cinebox_core::TorrentPlaybackPrefs;
use cinebox_core::i18n::Msg;
use cinebox_player::{ClickZone, Engine, SEEK_SECS, Track, click_zone};
use egui::{Align2, Rect, RichText, Sense, Ui, Vec2, ViewportCommand};
use egui_async::Bind;
use tracing::warn;

use crate::nav::NavAction;
use crate::screens::play::PlayRequest;
use crate::screens::torrents::TorrentFileRow;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::flyout;

use buffering::{Buffering, PreloadMeter};
use overlay::{Activity, FooterView};

struct PlayerState {
    title: String,
    hash: String,
    files: Vec<TorrentFileRow>,
    file_index: usize,
    backdrop_path: Option<String>,
    paused: bool,
    time: f64,
    duration: f64,
    error: Option<String>,
    muted: bool,
    volume: f64,
}

impl PlayerState {
    fn from_spec(spec: &LoadSpec) -> Self {
        Self {
            title: spec.title.clone(),
            hash: spec.hash.clone(),
            files: spec.files.clone(),
            file_index: spec.file_index,
            backdrop_path: spec.backdrop_path.clone(),
            paused: false,
            time: spec.resume_at,
            duration: 0.0,
            error: None,
            muted: false,
            volume: 100.0,
        }
    }

    #[must_use]
    fn has_next(&self) -> bool {
        self.file_index + 1 < self.files.len()
    }
}

enum PlayerPhase {
    Buffering(Buffering),
    Playing(PlayerState),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Popup {
    None,
    Settings(settings_popup::Page),
    Playlist,
    Volume,
}

/// Everything one load needs; the single path shared by start / next / prev / jump.
struct LoadSpec {
    title: String,
    hash: String,
    files: Vec<TorrentFileRow>,
    file_index: usize,
    resume_at: f64,
    backdrop_path: Option<String>,
}

pub struct PlayerScreen {
    phase: Option<PlayerPhase>,
    fullscreen: bool,
    was_maximized: bool,
    activity: Activity,
    popup: Popup,
    prefs: TorrentPlaybackPrefs,
    sub_scale: f64,
    sub_delay: f64,
    volume_dirty: bool,
}

impl Default for PlayerScreen {
    fn default() -> Self {
        Self {
            phase: None,
            fullscreen: false,
            was_maximized: false,
            activity: Activity::new(),
            popup: Popup::None,
            prefs: TorrentPlaybackPrefs::default(),
            sub_scale: 1.0,
            sub_delay: 0.0,
            volume_dirty: false,
        }
    }
}

impl PlayerScreen {
    pub fn start(&mut self, req: PlayRequest, svc: &mut Services, ctx: &egui::Context) {
        self.prefs = svc
            .db
            .as_ref()
            .and_then(|db| db.get_torrent_prefs(&req.hash).ok().flatten())
            .unwrap_or_default();
        self.sub_scale = 1.0;
        self.sub_delay = 0.0;
        self.activity.poke(ctx.input(|i| i.time));

        self.begin_load(
            svc,
            ctx,
            LoadSpec {
                title: req.title,
                hash: req.hash,
                files: req.files,
                file_index: req.file_index,
                resume_at: req.start,
                backdrop_path: req.backdrop_path,
            },
        );
    }

    pub fn stop(&mut self, svc: &mut Services, ctx: &egui::Context) {
        stop_engine(svc);

        self.phase = None;
        self.popup = Popup::None;

        if self.volume_dirty {
            svc.persist();
            self.volume_dirty = false;
        }

        self.set_fullscreen(ctx, false);
    }

    #[must_use]
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// Escape while the player is on screen: close a popup first, then exit
    /// fullscreen. `true` when consumed (navigation must not pop).
    pub fn consume_escape(&mut self, ctx: &egui::Context) -> bool {
        if self.popup != Popup::None {
            self.popup = Popup::None;
            return true;
        }

        if self.fullscreen {
            self.set_fullscreen(ctx, false);
            return true;
        }

        false
    }

    pub fn tick(&mut self, svc: &mut Services, ctx: &egui::Context) {
        self.sync_fullscreen(ctx);
        self.poll_buffering(svc);

        let go_next = {
            let Some(PlayerPhase::Playing(state)) = &mut self.phase else {
                return;
            };
            let Some(engine) = &svc.engine else {
                return;
            };
            let Ok(engine) = engine.lock() else {
                return;
            };

            let snap = engine.snapshot();
            drop(engine);

            state.paused = snap.paused;
            state.time = snap.time;
            state.duration = snap.duration;
            state.muted = snap.muted;
            state.volume = snap.volume;

            snap.eof && snap.duration > 1.0 && svc.settings.player.auto_next && state.has_next()
        };

        if go_next {
            self.next_file(svc, ctx);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        let now = ui.input(|i| i.time);
        self.update_activity(ui, now);
        self.handle_keys(ui, svc);

        let video = ui.available_rect_before_wrap();

        match &self.phase {
            None => {
                ui.label(RichText::new(Msg::LoadingCard.t()).color(theme.muted));
                None
            }
            Some(PlayerPhase::Buffering(_)) => {
                self.buffering_ui(ui, svc, theme, video);
                None
            }
            Some(PlayerPhase::Playing(_)) => {
                self.playing_ui(ui, svc, theme, video, now);
                None
            }
        }
    }
}

struct PlayingView {
    title: String,
    error: Option<String>,
    time: f64,
    duration: f64,
    paused: bool,
    muted: bool,
    volume: f64,
    file_count: usize,
    file_index: usize,
    has_next: bool,
}

impl PlayerScreen {
    fn buffering_ui(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme, video: Rect) {
        let Some(PlayerPhase::Buffering(state)) = &self.phase else {
            return;
        };

        let (rect, _) = ui.allocate_exact_size(video.size(), Sense::hover());
        buffering::paint(ui, rect, svc, theme, state);
        overlay::header(ui.ctx(), theme, rect, &state.title, 1.0);
    }

    fn playing_ui(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        video: Rect,
        now: f64,
    ) {
        let ctx = ui.ctx().clone();
        let popup_was_open = self.popup != Popup::None;

        let view = {
            let Some(PlayerPhase::Playing(state)) = &self.phase else {
                return;
            };

            PlayingView {
                title: state.title.clone(),
                error: state.error.clone(),
                time: state.time,
                duration: state.duration,
                paused: state.paused,
                muted: state.muted,
                volume: state.volume,
                file_count: state.files.len(),
                file_index: state.file_index,
                has_next: state.has_next(),
            }
        };

        let (rect, response) = ui.allocate_exact_size(video.size(), Sense::click());
        if let Some(error) = view.error.as_deref() {
            ui.painter().rect_filled(rect, 0.0, theme.video_bg);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                error,
                theme.ui_font(theme.text_body),
                theme.err,
            );
        } else {
            paint_video(ui, rect, svc.engine.clone(), theme);
        }

        let mut seek_rel = None;
        let mut toggle = false;
        let video_clicked = view.error.is_none() && !popup_was_open && response.clicked();

        if video_clicked && let Some(pos) = response.interact_pointer_pos() {
            let x_ratio = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            match click_zone(x_ratio) {
                ClickZone::SeekBack => seek_rel = Some(-SEEK_SECS),
                ClickZone::Pause => toggle = true,
                ClickZone::SeekFwd => seek_rel = Some(SEEK_SECS),
            }
        }

        if popup_was_open {
            self.activity.poke(now);
        }

        let alpha = self.activity.visual_t(now);
        let header_rect = overlay::header(&ctx, theme, rect, &view.title, alpha);

        let footer_view = FooterView {
            time: view.time,
            duration: view.duration,
            paused: view.paused,
            muted: view.muted,
            volume: view.volume,
            file_count: view.file_count,
            file_index: view.file_index,
            has_next: view.has_next,
            fullscreen: self.fullscreen,
        };
        let footer = overlay::footer(&ctx, theme, rect, &footer_view, alpha);

        let pointer = ctx.pointer_latest_pos();
        let over_chrome = pointer.is_some_and(|pos| {
            header_rect.is_some_and(|header| header.contains(pos)) || footer.rect.contains(pos)
        });
        if over_chrome {
            self.activity.poke(now);
        }

        self.popups(&ctx, svc, theme, &footer);

        if let Some(to) = footer.seek_to {
            self.seek_abs(svc, to);
        }
        if let Some(delta) = footer.seek_rel.or(seek_rel) {
            self.seek(svc, delta);
        }
        if footer.toggle_pause || toggle {
            self.toggle(svc);
        }
        if footer.prev {
            self.prev_file(svc, &ctx);
        }
        if footer.next {
            self.next_file(svc, &ctx);
        }
        if footer.playlist_clicked {
            self.toggle_popup(Popup::Playlist);
        }
        if footer.settings_clicked {
            self.toggle_popup(Popup::Settings(settings_popup::Page::Root));
        }
        if footer.volume_clicked {
            self.toggle_mute(svc);
        }
        if footer.fullscreen_clicked {
            let next = !self.fullscreen;
            self.set_fullscreen(&ctx, next);
        }
        if footer.volume_hovered && self.popup == Popup::None {
            self.popup = Popup::Volume;
        }

        if alpha > 0.0 && alpha < 1.0 {
            ctx.request_repaint();
        } else if alpha > 0.0 {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    fn popups(
        &mut self,
        ctx: &egui::Context,
        svc: &mut Services,
        theme: &Theme,
        footer: &overlay::FooterOut,
    ) {
        match self.popup {
            Popup::None => {}
            Popup::Settings(page) => {
                self.settings_popup(ctx, svc, theme, footer.settings_rect, page)
            }
            Popup::Playlist => self.playlist_popup(ctx, svc, theme, footer.playlist_rect),
            Popup::Volume => self.volume_popup(ctx, svc, theme, footer.volume_rect),
        }
    }

    fn settings_popup(
        &mut self,
        ctx: &egui::Context,
        svc: &mut Services,
        theme: &Theme,
        anchor: Rect,
        page: settings_popup::Page,
    ) {
        let tracks = engine_tracks(svc);
        let view = settings_popup::View {
            page,
            tracks: &tracks,
            prefs: self.prefs,
            sub_scale: self.sub_scale,
            sub_delay: self.sub_delay,
        };

        let mut out = settings_popup::Out::default();
        let pad = egui::Margin::same(flyout::PAD);
        let fly = flyout::show(
            ctx,
            "player-settings-flyout",
            anchor,
            theme,
            280.0,
            pad,
            |ui, theme| {
                out = settings_popup::paint(ui, theme, &view);
            },
        );

        if let Some(next) = out.page {
            self.popup = Popup::Settings(next);
        }

        let mut dirty = false;

        if let Some(scale) = out.scale {
            self.prefs.scale = scale;
            with_engine(svc, |engine| {
                let _ = engine.set_scale(scale);
            });
            dirty = true;
        }

        if let Some(speed) = out.speed {
            self.prefs.speed = speed;
            with_engine(svc, |engine| {
                let _ = engine.set_speed(speed);
            });
            dirty = true;
        }

        if let Some(id) = out.audio {
            self.prefs.aid = id;
            with_engine(svc, |engine| {
                let _ = engine.select_audio(id);
            });
            dirty = true;
        }

        if let Some(sub) = out.sub {
            self.prefs.sid = sub.unwrap_or(0);
            with_engine(svc, |engine| {
                let _ = engine.select_sub(sub);
            });
            dirty = true;
        }

        if let Some(scale) = out.sub_scale {
            self.sub_scale = scale;
            with_engine(svc, |engine| {
                let _ = engine.set_sub_scale(scale);
            });
        }

        if let Some(delay) = out.sub_delay {
            self.sub_delay = delay;
            with_engine(svc, |engine| {
                let _ = engine.set_sub_delay(delay);
            });
        }

        if dirty {
            self.save_prefs(svc);
        }

        if fly.dismissed {
            self.popup = Popup::None;
        }
    }

    fn playlist_popup(
        &mut self,
        ctx: &egui::Context,
        svc: &mut Services,
        theme: &Theme,
        anchor: Rect,
    ) {
        let mut jump = None;

        let dismissed = {
            let Some(PlayerPhase::Playing(state)) = &self.phase else {
                return;
            };
            let files = &state.files;
            let current = state.file_index;

            // Bottom margin 0: the list runs flush to the frame edge, covered
            // by the flyout's own bottom-up shadow.
            let pad = egui::Margin {
                left: flyout::PAD,
                right: flyout::PAD,
                top: flyout::PAD,
                bottom: 0,
            };
            let fly = flyout::show(
                ctx,
                "player-playlist-flyout",
                anchor,
                theme,
                380.0,
                pad,
                |ui, theme| {
                    jump = playlist_popup::paint(ui, theme, svc, files, current);
                },
            );

            fly.dismissed
        };

        if let Some(index) = jump {
            self.popup = Popup::None;
            self.jump_to_file(svc, ctx, index);
            return;
        }

        if dismissed {
            self.popup = Popup::None;
        }
    }

    fn volume_popup(
        &mut self,
        ctx: &egui::Context,
        svc: &mut Services,
        theme: &Theme,
        anchor: Rect,
    ) {
        let mut level = svc.settings.player.volume;
        let mut changed = false;

        let pad = egui::Margin::same(flyout::PAD);
        let fly = flyout::show(
            ctx,
            "player-volume-flyout",
            anchor,
            theme,
            volume::WIDTH,
            pad,
            |ui, theme| {
                changed = volume::slider(ui, theme, &mut level);
            },
        );

        if changed {
            let level = level.clamp(0.0, 100.0);
            svc.settings.player.volume = level;
            with_engine(svc, |engine| {
                let _ = engine.set_volume(level);
            });
            self.volume_dirty = true;
        }

        let pointer = ctx.pointer_latest_pos();
        let over = pointer.is_some_and(|pos| {
            fly.rect.expand(8.0).contains(pos) || anchor.expand(8.0).contains(pos)
        });

        if over {
            return;
        }

        self.popup = Popup::None;

        if self.volume_dirty {
            svc.persist();
            self.volume_dirty = false;
        }
    }

    fn toggle_popup(&mut self, popup: Popup) {
        let same = std::mem::discriminant(&self.popup) == std::mem::discriminant(&popup);
        if same {
            self.popup = Popup::None;
            return;
        }

        self.popup = popup;
    }

    fn update_activity(&mut self, ui: &Ui, now: f64) {
        let interacted = ui.input(|i| {
            i.pointer.delta() != Vec2::ZERO
                || i.pointer.any_down()
                || i.smooth_scroll_delta != Vec2::ZERO
                || i.events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Key { .. }))
        });

        if interacted {
            self.activity.poke(now);
        }
    }

    fn handle_keys(&mut self, ui: &Ui, svc: &Services) {
        if !matches!(self.phase, Some(PlayerPhase::Playing(_))) {
            return;
        }

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
}

impl PlayerScreen {
    /// The one load path: resolves the file, optionally waits for preload.
    fn begin_load(&mut self, svc: &mut Services, ctx: &egui::Context, spec: LoadSpec) {
        self.popup = Popup::None;
        self.activity.poke(ctx.input(|i| i.time));

        if !svc.settings.torrserver.wait_preload {
            self.phase = Some(PlayerPhase::Playing(PlayerState::from_spec(&spec)));
            self.load_current(svc);
            return;
        }

        stop_engine(svc);

        let Some(file) = spec.files.get(spec.file_index) else {
            return;
        };

        let meter = PreloadMeter::new();
        let mut job: Bind<(), String> = Bind::new(true);
        job.set_abort(true);

        let settings = svc.settings.clone();
        let live = meter.clone();
        let repaint = ctx.clone();
        let path = file.path.clone();
        let hash = spec.hash.clone();
        let file_id = file.id;

        job.request(async move {
            crate::jobs::wait_stream(settings, path, hash, file_id, move |event| {
                live.on_event(event);
                repaint.request_repaint();
            })
            .await
        });

        self.phase = Some(PlayerPhase::Buffering(Buffering {
            title: spec.title,
            backdrop_path: spec.backdrop_path,
            hash: spec.hash,
            files: spec.files,
            file_index: spec.file_index,
            resume_at: spec.resume_at,
            meter,
            job,
        }));
    }

    fn poll_buffering(&mut self, svc: &mut Services) {
        let result = {
            let Some(PlayerPhase::Buffering(state)) = &mut self.phase else {
                return;
            };

            match state.job.read() {
                None => return,
                Some(Ok(())) => Ok(()),
                Some(Err(error)) => Err(error.clone()),
            }
        };

        let Some(PlayerPhase::Buffering(buffered)) = self.phase.take() else {
            return;
        };

        let mut state = PlayerState::from_spec(&LoadSpec {
            title: buffered.title,
            hash: buffered.hash,
            files: buffered.files,
            file_index: buffered.file_index,
            resume_at: buffered.resume_at,
            backdrop_path: buffered.backdrop_path,
        });

        if let Err(error) = result {
            state.error = Some(error);
        }

        let failed = state.error.is_some();
        self.phase = Some(PlayerPhase::Playing(state));

        if !failed {
            self.load_current(svc);
        }
    }

    fn load_current(&mut self, svc: &mut Services) {
        let prefs = self.prefs;
        let sub_scale = self.sub_scale;
        let sub_delay = self.sub_delay;
        let volume = svc.settings.player.volume;

        let Some(PlayerPhase::Playing(state)) = &mut self.phase else {
            return;
        };

        let Some(engine) = &svc.engine else {
            state.error = Some(Msg::MpvRenderFailed.t().to_owned());
            return;
        };

        let Some(file) = state.files.get(state.file_index) else {
            return;
        };

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

        let header = cinebox_torrserver::mpv_http_header_fields(
            &svc.settings.torrserver.username,
            svc.settings.torrserver.password.expose(),
        );
        let opts = cinebox_player::PlayOpts {
            http_header_fields: header.as_deref(),
            loudnorm: svc.settings.player.loudnorm,
            start_seconds: state.time,
        };

        let Ok(engine) = engine.lock() else {
            return;
        };

        if let Err(error) = engine.load(&url, opts) {
            state.error = Some(error.to_string());
            return;
        }

        state.error = None;

        let _ = engine.set_scale(prefs.scale);
        let _ = engine.set_speed(prefs.speed);
        let _ = engine.set_volume(volume);
        let _ = engine.set_sub_scale(sub_scale);
        let _ = engine.set_sub_delay(sub_delay);

        if prefs.aid > 0 {
            let _ = engine.select_audio(prefs.aid);
        }

        if prefs.sid > 0 {
            let _ = engine.select_sub(Some(prefs.sid));
        } else if prefs.sid == 0 {
            let _ = engine.select_sub(None);
        }
    }

    fn next_file(&mut self, svc: &mut Services, ctx: &egui::Context) {
        let index = {
            let Some(PlayerPhase::Playing(state)) = &self.phase else {
                return;
            };
            if !state.has_next() {
                return;
            }

            state.file_index + 1
        };

        self.jump_to_file(svc, ctx, index);
    }

    fn prev_file(&mut self, svc: &mut Services, ctx: &egui::Context) {
        let index = {
            let Some(PlayerPhase::Playing(state)) = &self.phase else {
                return;
            };
            let Some(index) = state.file_index.checked_sub(1) else {
                return;
            };

            index
        };

        self.jump_to_file(svc, ctx, index);
    }

    fn jump_to_file(&mut self, svc: &mut Services, ctx: &egui::Context, index: usize) {
        let spec = {
            let Some(phase) = &self.phase else {
                return;
            };

            let (hash, files, backdrop_path, current) = match phase {
                PlayerPhase::Playing(state) => (
                    &state.hash,
                    &state.files,
                    &state.backdrop_path,
                    Some(state.file_index),
                ),
                PlayerPhase::Buffering(state) => {
                    (&state.hash, &state.files, &state.backdrop_path, None)
                }
            };

            if current == Some(index) {
                return;
            }

            let Some(file) = files.get(index) else {
                return;
            };

            LoadSpec {
                title: file.title.clone(),
                hash: hash.clone(),
                files: files.clone(),
                file_index: index,
                resume_at: file.timecode,
                backdrop_path: backdrop_path.clone(),
            }
        };

        self.begin_load(svc, ctx, spec);
    }

    fn save_prefs(&self, svc: &Services) {
        let Some(db) = &svc.db else {
            return;
        };

        let hash = match &self.phase {
            Some(PlayerPhase::Playing(state)) => &state.hash,
            Some(PlayerPhase::Buffering(state)) => &state.hash,
            None => return,
        };

        if let Err(error) = db.put_torrent_prefs(hash, &self.prefs) {
            warn!(%error, "failed to save torrent playback prefs");
        }
    }

    fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        if self.fullscreen == on {
            return;
        }

        self.fullscreen = on;
        if on {
            // A maximized undecorated window overhangs the screen edges on
            // Windows; going borderless-fullscreen from it keeps that stale
            // geometry (footer lands below the screen). Un-maximize first.
            self.was_maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
            if self.was_maximized {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
            }
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
            return;
        }
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        if self.was_maximized {
            self.was_maximized = false;
            ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
        }
    }

    fn sync_fullscreen(&mut self, ctx: &egui::Context) {
        if !self.fullscreen {
            return;
        }

        let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
        if maximized {
            self.was_maximized = true;
            ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
        }

        let fullscreen = ctx.input(|i| i.viewport().fullscreen).unwrap_or(true);
        if !fullscreen {
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
        }
    }

    fn toggle(&mut self, svc: &Services) {
        let paused = with_engine(svc, |engine| engine.toggle_pause().ok()).flatten();
        let Some(paused) = paused else {
            return;
        };

        if let Some(PlayerPhase::Playing(state)) = &mut self.phase {
            state.paused = paused;
        }
    }

    fn toggle_mute(&mut self, svc: &Services) {
        let Some(PlayerPhase::Playing(state)) = &mut self.phase else {
            return;
        };

        let next = !state.muted;
        let ok = with_engine(svc, |engine| engine.set_mute(next).is_ok()).unwrap_or(false);
        if ok {
            state.muted = next;
        }
    }

    fn seek(&self, svc: &Services, delta: f64) {
        with_engine(svc, |engine| {
            let _ = engine.seek(delta);
        });
    }

    fn seek_abs(&mut self, svc: &Services, to: f64) {
        with_engine(svc, |engine| {
            let _ = engine.seek_abs(to);
        });

        if let Some(PlayerPhase::Playing(state)) = &mut self.phase {
            state.time = to;
        }
    }
}

fn with_engine<R>(svc: &Services, f: impl FnOnce(&Engine) -> R) -> Option<R> {
    let engine = svc.engine.as_ref()?;
    let engine = engine.lock().ok()?;

    Some(f(&engine))
}

fn stop_engine(svc: &Services) {
    with_engine(svc, Engine::stop);
}

fn engine_tracks(svc: &Services) -> Vec<Track> {
    with_engine(svc, Engine::track_list).unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: i32) -> TorrentFileRow {
        TorrentFileRow {
            id,
            path: format!("Season 1/E{id:02}.mkv"),
            length: 1000,
            timecode: 0.0,
            number: id as u32,
            season: Some(1),
            episode: Some(id as u32),
            title: format!("Episode {id}"),
            still_url: None,
            runtime_minutes: Some(40),
            air_date: None,
        }
    }

    fn spec(file_index: usize, count: i32) -> LoadSpec {
        LoadSpec {
            title: String::from("Episode"),
            hash: String::from("deadbeef"),
            files: (1..=count).map(file).collect(),
            file_index,
            resume_at: 12.0,
            backdrop_path: None,
        }
    }

    #[test]
    fn state_from_spec_resumes_at_timecode() {
        let state = PlayerState::from_spec(&spec(0, 3));

        assert!((state.time - 12.0).abs() < f64::EPSILON);
        assert!(state.has_next());

        let last = PlayerState::from_spec(&spec(2, 3));
        assert!(!last.has_next());
    }

    #[test]
    fn toggle_popup_flips_same_kind_and_swaps_other() {
        let mut screen = PlayerScreen::default();

        screen.toggle_popup(Popup::Playlist);
        assert!(screen.popup == Popup::Playlist);

        screen.toggle_popup(Popup::Playlist);
        assert!(screen.popup == Popup::None);

        screen.toggle_popup(Popup::Settings(settings_popup::Page::Root));
        screen.toggle_popup(Popup::Settings(settings_popup::Page::Speed));
        assert!(screen.popup == Popup::None, "same discriminant toggles off");

        screen.toggle_popup(Popup::Settings(settings_popup::Page::Root));
        screen.toggle_popup(Popup::Playlist);
        assert!(screen.popup == Popup::Playlist);
    }

    #[test]
    fn escape_closes_popup_before_leaving_fullscreen() {
        let ctx = egui::Context::default();
        let mut screen = PlayerScreen {
            fullscreen: true,
            popup: Popup::Playlist,
            ..PlayerScreen::default()
        };

        assert!(screen.consume_escape(&ctx));
        assert!(screen.popup == Popup::None);
        assert!(screen.is_fullscreen());

        assert!(screen.consume_escape(&ctx));
        assert!(!screen.is_fullscreen());

        assert!(!screen.consume_escape(&ctx));
    }
}
