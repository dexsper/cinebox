//! In-window player: full-bleed video, floating auto-hiding chrome, popups,
//! true OS fullscreen, and a TorrServer buffering phase.

mod buffering;
mod input;
mod overlay;
mod playlist_popup;
mod progress;
mod session;
mod settings_popup;
mod skip;
mod skip_overlay;
mod volume;

use std::sync::Arc;
use std::time::{Duration, Instant};

use cinebox_core::{MediaKind, TmdbId, TorrentPlaybackPrefs, VideoScale};
use cinebox_player::{
    Area, ClickZone, Failure, Media, Player, SEEK_SECS, Track, VideoOutput, click_zone, fit_video,
};
use egui::{Align, Align2, Color32, Rect, RichText, Sense, Ui, pos2};
use egui_async::Bind;
use rust_i18n::t;
use tracing::{info, warn};

use crate::app::nav::NavAction;
use crate::i18n::errors::UserError;
use crate::platform::MediaCommand;
use crate::screens::play::{PlayRequest, PlaySource, WatchCard};
use crate::screens::torrents::TorrentFileRow;
use crate::services::jobs::{self, JobError};
use crate::services::{Services, db_block_on};
use crate::theme::Theme;
use crate::widgets::flyout;

use buffering::{Buffering, PreloadMeter};
use overlay::{Activity, FooterView};

struct PlayerState {
    card: WatchCard,
    title: String,
    source: PlaySource,
    backdrop_path: Option<String>,
    paused: bool,
    time: f64,
    duration: f64,
    error: Option<String>,
    muted: bool,
    volume: f64,
    video_size: Option<[u32; 2]>,
    /// Drawn by the app when the player leaves subtitles to it.
    subtitle: Option<String>,
    loaded_at: Instant,
    decoder: Option<cinebox_player::VideoDecoder>,
}

impl PlayerState {
    fn from_spec(spec: &LoadSpec) -> Self {
        Self {
            card: spec.card.clone(),
            title: spec.title.clone(),
            source: spec.source.clone(),
            backdrop_path: spec.backdrop_path.clone(),
            paused: false,
            time: spec.source.start_seconds(),
            duration: 0.0,
            error: None,
            muted: false,
            volume: 100.0,
            video_size: None,
            subtitle: None,
            loaded_at: Instant::now(),
            decoder: None,
        }
    }

    #[must_use]
    fn has_next(&self) -> bool {
        self.source.has_next()
    }

    #[must_use]
    fn files(&self) -> &[TorrentFileRow] {
        self.source.files()
    }

    #[must_use]
    fn file_index(&self) -> usize {
        self.source.file_index()
    }

    #[must_use]
    fn torrent_hash(&self) -> Option<&str> {
        self.source.torrent_hash()
    }

    #[must_use]
    fn is_youtube(&self) -> bool {
        self.source.is_youtube()
    }
}

/// What [`PlayerScreen::stop`] leaves behind.
pub struct Played {
    pub source: PlaySource,
    /// The database write of the progress, still running in the background.
    pub saved: Option<tokio::task::JoinHandle<()>>,
}

enum PlayerPhase {
    Buffering(Buffering),
    Playing(PlayerState),
}

impl PlayerPhase {
    fn into_source(self) -> PlaySource {
        match self {
            Self::Playing(state) => state.source,
            Self::Buffering(state) => PlaySource::Torrent {
                hash: state.hash,
                files: state.files,
                file_index: state.file_index,
                start: state.resume_at,
            },
        }
    }
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
    card: WatchCard,
    title: String,
    source: PlaySource,
    backdrop_path: Option<String>,
}

/// The video `PaintCallback` closure, reused across frames while the player
/// stays the same.
struct VideoCallback {
    player: Arc<dyn Player>,
    callback: Arc<egui_glow::CallbackFn>,
}

pub struct PlayerScreen {
    phase: Option<PlayerPhase>,
    fullscreen: bool,
    was_maximized: bool,
    activity: Activity,
    /// TV: the D-pad was on the transport controls, so Back hides them first.
    controls_focused: bool,
    focus_play: bool,
    popup: Popup,
    /// See [`settings_popup::View::left`].
    settings_left: Option<settings_popup::Page>,
    playlist_scroll: bool,
    prefs: TorrentPlaybackPrefs,
    sub_scale: f64,
    sub_delay: f64,
    volume_dirty: bool,
    progress_saved_at: Option<Instant>,
    viewed_job: Bind<(), JobError>,
    skip_state: skip::SkipState,
    skip_save_job: Bind<(), JobError>,
    video_cb: Option<VideoCallback>,
    session: session::MediaSession,
}

impl Default for PlayerScreen {
    fn default() -> Self {
        Self {
            phase: None,
            fullscreen: false,
            was_maximized: false,
            activity: Activity::new(),
            controls_focused: false,
            focus_play: false,
            popup: Popup::None,
            settings_left: None,
            playlist_scroll: false,
            prefs: TorrentPlaybackPrefs::default(),
            sub_scale: 1.0,
            sub_delay: 0.0,
            volume_dirty: false,
            progress_saved_at: None,
            viewed_job: Bind::new(true),
            skip_state: skip::SkipState::default(),
            skip_save_job: Bind::new(true),
            video_cb: None,
            session: session::MediaSession::default(),
        }
    }
}

impl PlayerScreen {
    pub fn start(&mut self, req: PlayRequest, svc: &mut Services, ctx: &egui::Context) {
        self.prefs = Default::default();

        if let Some(hash) = req.source.torrent_hash() {
            self.prefs = svc
                .db
                .as_ref()
                .and_then(|db| db_block_on(db.get_torrent_prefs(hash)).ok().flatten())
                .unwrap_or_default();
        }

        self.sub_scale = 1.0;
        self.sub_delay = 0.0;
        self.activity.poke(ctx.input(|i| i.time));
        crate::platform::device(ctx).keep_screen_on(true);

        self.begin_load(
            svc,
            ctx,
            LoadSpec {
                card: req.card,
                title: req.title,
                source: req.source,
                backdrop_path: req.backdrop_path,
            },
        );
    }

    /// Saves progress and stops. Returns the source it stopped on, with the
    /// timecodes just saved.
    pub fn stop(&mut self, svc: &mut Services, ctx: &egui::Context) -> Option<Played> {
        let saved = self.save_progress(svc, true);
        stop_player(svc);

        self.abort_buffering();
        self.skip_state.reset();
        let played = self.phase.take().map(|phase| Played {
            source: phase.into_source(),
            saved,
        });
        self.popup = Popup::None;

        if self.volume_dirty {
            svc.persist();
            self.volume_dirty = false;
        }

        self.set_fullscreen(ctx, false);
        crate::platform::device(ctx).keep_screen_on(false);
        self.session.clear(ctx);

        played
    }

    /// Pause without toggling (the app went to the background).
    pub fn pause(&mut self, svc: &Services) {
        let playing = matches!(&self.phase, Some(PlayerPhase::Playing(state)) if !state.paused);
        if playing {
            self.toggle(svc);
        }
    }

    fn resume(&mut self, svc: &Services) {
        let paused = matches!(&self.phase, Some(PlayerPhase::Playing(state)) if state.paused);
        if paused {
            self.toggle(svc);
        }
    }

    /// Transport control from outside the player UI.
    pub fn apply(&mut self, command: MediaCommand, svc: &mut Services, ctx: &egui::Context) {
        match command {
            MediaCommand::Play => self.resume(svc),
            MediaCommand::Pause | MediaCommand::Stop => self.pause(svc),
            MediaCommand::PlayPause => self.toggle(svc),
            MediaCommand::SeekTo(secs) => self.seek_abs(svc, secs),
            MediaCommand::SeekBy(secs) => self.seek(svc, secs),
            MediaCommand::Next => self.next_file(svc, ctx),
            MediaCommand::Previous => self.prev_file(svc, ctx),
        }

        if pauses(command) {
            self.focus_play_from_remote(ctx);
        }

        self.activity.poke(ctx.input(|i| i.time));
    }

    fn focus_play_from_remote(&mut self, ctx: &egui::Context) {
        if crate::platform::profile(ctx).is_directional() {
            self.focus_play = true;
        }
    }

    /// Video with nothing of the app's chrome around it. Where the OS owns the
    /// window there is no fullscreen to toggle: the player always fills it.
    #[must_use]
    pub fn fills_screen(&self, profile: crate::platform::Profile) -> bool {
        if !profile.is_desktop_window() {
            return true;
        }

        self.fullscreen
    }

    pub fn tick(&mut self, svc: &mut Services, ctx: &egui::Context) {
        self.sync_fullscreen(ctx);
        self.poll_buffering(svc);

        let (go_next, stall) = {
            let Some(PlayerPhase::Playing(state)) = &mut self.phase else {
                return;
            };

            let Some(player) = &svc.player else {
                return;
            };

            let snap = player.snapshot();
            // Logged on every change: the player may fall back to software mid-stream.
            let decoder = player.video_decoder();
            if decoder.is_some() && decoder != state.decoder {
                if let Some(d) = &decoder {
                    info!(hwdec = %d.hwdec, codec = %d.codec, pixel_format = %d.pixel_format, "video decoder");
                }
                state.decoder = decoder;
            }

            state.paused = snap.paused;
            state.time = snap.time;
            state.duration = snap.duration;
            state.muted = snap.muted;
            state.volume = snap.volume;
            state.video_size = snap.video_size;
            state.subtitle = snap.subtitle;

            if let Some(failure) = &snap.failure {
                if state.error.is_none() {
                    warn!(?failure, "playback failed");
                    state.error = Some(failure_message(failure));
                }
            }

            let started = http_stream_started(snap.time, snap.duration);
            let waiting = state.is_youtube() && state.error.is_none() && !started;
            let stall = waiting && state.loaded_at.elapsed() > STREAM_STALL;

            if stall {
                warn!("http stream did not start");
                state.error = Some(t!("player.stream_stalled").into_owned());
            }

            let eof_ready = snap.eof && snap.duration > 1.0;
            let go_next = eof_ready && svc.settings.player.auto_next && state.has_next();

            (go_next, stall)
        };

        if stall {
            stop_player(svc);
        }

        let _ = self.viewed_job.read();
        self.save_progress(svc, false);

        if go_next {
            self.next_file(svc, ctx);
        }

        if let Some(PlayerPhase::Playing(state)) = &self.phase {
            self.session.publish(svc, state, ctx);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        let now = ui.input(|i| i.time);
        self.update_activity(ui, now);
        self.handle_keys(ui, svc);

        let video = ui.available_rect_before_wrap();

        match &self.phase {
            None => {
                ui.painter().rect_filled(video, 0.0, theme.video_bg);
                ui.label(RichText::new(t!("common.loading").as_ref()).color(theme.muted));
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
    video_size: Option<[u32; 2]>,
    subtitle: Option<String>,
    file_count: usize,
    file_index: usize,
    has_next: bool,
    kind: MediaKind,
    tmdb_id: TmdbId,
    season: Option<u32>,
    episode: Option<u32>,
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

            let cur_file = state.files().get(state.file_index());
            let season = cur_file.and_then(|f| f.season);
            let episode = cur_file.and_then(|f| f.episode);

            PlayingView {
                title: state.title.clone(),
                error: state.error.clone(),
                time: state.time,
                duration: state.duration,
                paused: state.paused,
                muted: state.muted,
                volume: state.volume,
                video_size: state.video_size,
                subtitle: state.subtitle.clone(),
                file_count: state.files().len(),
                file_index: state.file_index(),
                has_next: state.has_next(),
                kind: state.card.kind,
                tmdb_id: state.card.id,
                season,
                episode,
            }
        };

        let (rect, _) = ui.allocate_exact_size(video.size(), Sense::hover());
        let response = ui.interact(rect, input::video_id(), Sense::click());
        let show_controls = self.dpad_video_focus(ui, &response, popup_was_open, now);
        if show_controls {
            self.focus_play = true;
        }

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
            let frame = VideoFrame {
                scale: self.prefs.scale,
                size: view.video_size,
                subtitle: view.subtitle.as_deref(),
                subtitle_scale: self.sub_scale,
            };
            paint_video(
                ui,
                rect,
                svc.player.clone(),
                theme,
                &mut self.video_cb,
                &frame,
            );
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
        } else if video_clicked {
            // OK on the remote: no pointer, so no click zone.
            toggle = true;
            self.focus_play_from_remote(&ctx);
        }

        if popup_was_open {
            self.activity.poke(now);
        }

        let alpha = self.activity.visual_t(now);
        let header_rect = overlay::header(&ctx, theme, rect, &view.title, alpha);

        let profile = crate::platform::profile(&ctx);
        let footer_view = FooterView {
            time: view.time,
            duration: view.duration,
            paused: view.paused,
            muted: view.muted,
            volume: view.volume,
            volume_control: svc.player_features().volume,
            file_count: view.file_count,
            file_index: view.file_index,
            has_next: view.has_next,
            fullscreen: self.fullscreen,
            can_fullscreen: profile.is_desktop_window(),
            directional: profile.is_directional(),
        };
        let footer = overlay::footer(&ctx, theme, rect, &footer_view, alpha);
        if std::mem::take(&mut self.focus_play) {
            ctx.memory_mut(|mem| mem.request_focus(footer.play_id));
        }

        let focused = ctx.memory(|mem| mem.focused());
        self.controls_focused = focused.is_some_and(|id| id != input::video_id());

        let pointer = ctx.pointer_latest_pos();
        let over_chrome = pointer.is_some_and(|pos| {
            header_rect.is_some_and(|header| header.contains(pos)) || footer.rect.contains(pos)
        });
        if over_chrome {
            self.activity.poke(now);
        }

        self.popups(&ctx, svc, theme, &footer);

        // Skip-segment overlay — tick state machine, then render banner.
        let _ = self.skip_save_job.read();

        let skip_seek = self.skip_state.tick(
            now,
            view.time,
            view.duration,
            view.paused,
            skip::SkipTarget {
                kind: view.kind,
                tmdb_id: view.tmdb_id,
                season: view.season,
                episode: view.episode,
            },
            svc,
        );

        if let Some(target) = skip_seek {
            self.skip_or_next(svc, &ctx, target, view.duration, view.has_next);
        }

        let banner = skip_overlay::show(&self.skip_state, &ctx, theme, rect, footer.seek_rect, now);

        if banner.skip_clicked {
            if let Some((target, ty)) = self.skip_state.on_skip(view.duration) {
                self.skip_or_next(svc, &ctx, target, view.duration, view.has_next);

                if let Some(db) = svc.db.clone() {
                    let job = jobs::save_skip_choice(db, view.kind, view.tmdb_id, ty, true);
                    self.skip_save_job.request(job);
                }
            }
        }

        if banner.cancel_clicked {
            let disarmed_ty = self.skip_state.on_cancel();

            if let (Some(ty), Some(db)) = (disarmed_ty, svc.db.clone()) {
                let job = jobs::save_skip_choice(db, view.kind, view.tmdb_id, ty, false);
                self.skip_save_job.request(job);
            }
        }

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
        let tracks = player_tracks(svc);
        let view = settings_popup::View {
            page,
            features: svc.player_features(),
            tracks: &tracks,
            prefs: self.prefs,
            sub_scale: self.sub_scale,
            sub_delay: self.sub_delay,
            left: self.settings_left,
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
            self.turn_settings_page(page, next);
        }

        let mut dirty = false;

        if let Some(scale) = out.scale {
            self.prefs.scale = scale;
            with_player(svc, |player| {
                log_player("set_scale", player.set_scale(scale));
            });
            dirty = true;
        }

        if let Some(speed) = out.speed {
            self.prefs.speed = speed;
            with_player(svc, |player| {
                log_player("set_speed", player.set_speed(speed));
            });
            dirty = true;
        }

        if let Some(id) = out.audio {
            self.prefs.aid = id;
            with_player(svc, |player| {
                log_player("select_audio", player.select_audio(id));
            });
            dirty = true;
        }

        if let Some(sub) = out.sub {
            self.prefs.sid = sub.unwrap_or(0);
            with_player(svc, |player| {
                log_player("select_subtitle", player.select_subtitle(sub));
            });
            dirty = true;
        }

        if let Some(scale) = out.sub_scale {
            self.sub_scale = scale;
            with_player(svc, |player| {
                log_player("set_subtitle_scale", player.set_subtitle_scale(scale));
            });
        }

        if let Some(delay) = out.sub_delay {
            self.sub_delay = delay;
            with_player(svc, |player| {
                log_player("set_subtitle_delay", player.set_subtitle_delay(delay));
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
        let mut scroll_to_current = self.playlist_scroll;

        let dismissed = {
            let Some(PlayerPhase::Playing(state)) = &self.phase else {
                return;
            };
            let files = state.files();
            let current = state.file_index();

            let pad = egui::Margin::same(flyout::PAD);
            let fly = flyout::show(
                ctx,
                "player-playlist-flyout",
                anchor,
                theme,
                380.0,
                pad,
                |ui, theme| {
                    jump = playlist_popup::paint(
                        ui,
                        theme,
                        svc,
                        files,
                        current,
                        &mut scroll_to_current,
                    );
                },
            );

            fly.dismissed
        };

        self.playlist_scroll = scroll_to_current;

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
            with_player(svc, |player| {
                log_player("set_volume", player.set_volume(level));
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

        if matches!(popup, Popup::Playlist) {
            self.playlist_scroll = true;
        }

        self.settings_left = None;
        self.popup = popup;
    }

    fn turn_settings_page(&mut self, from: settings_popup::Page, to: settings_popup::Page) {
        self.settings_left = Some(from);
        self.popup = Popup::Settings(to);
    }
}

/// Resumes this close to the start keep the stock head preload: its window
/// usually reaches the seek target, and a dedicated mid-file wait isn't worth it.
const RESUME_PRELOAD_MIN_SECS: f64 = 60.0;
const STREAM_STALL: Duration = Duration::from_secs(20);

/// The player can report `duration == 0` while the clock already advances.
fn http_stream_started(time: f64, duration: f64) -> bool {
    if time > 0.0 {
        return true;
    }

    duration > 0.0
}

/// Approximate byte offset of `resume_at`, assuming constant bitrate.
///
/// `None` for fresh starts or when TMDB gave no runtime. The caller then
/// falls back to the stock head preload.
fn resume_bytes_for_file(file: &TorrentFileRow, resume_at: f64) -> Option<u64> {
    if resume_at <= RESUME_PRELOAD_MIN_SECS {
        return None;
    }

    if file.length == 0 {
        return None;
    }

    let mins = file.runtime_minutes.filter(|mins| *mins > 0)?;
    let duration = f64::from(mins) * 60.0;
    let frac = (resume_at / duration).clamp(0.0, 0.99);

    Some((frac * file.length as f64) as u64)
}

struct LoadMedia {
    url: String,
    headers: Vec<String>,
    audio: Option<String>,
    proxy: Option<String>,
    start: f64,
}

fn load_args(state: &PlayerState, svc: &Services) -> Result<LoadMedia, String> {
    match &state.source {
        PlaySource::Youtube {
            video_url,
            audio_url,
            http_header_fields,
        } => {
            let net = jobs::net_config(&svc.settings);
            let proxy = cinebox_net::http_proxy_url(&net);
            if net.use_system_proxy && proxy.is_none() {
                warn!("system proxy is on but no http proxy url was found for the player");
            }

            tracing::debug!(has_proxy = proxy.is_some(), "youtube load");

            Ok(LoadMedia {
                url: video_url.clone(),
                headers: http_header_fields.clone(),
                audio: audio_url.clone(),
                proxy,
                start: 0.0,
            })
        }
        PlaySource::Torrent {
            hash,
            files,
            file_index,
            ..
        } => {
            let Some(file) = files.get(*file_index) else {
                return Err(t!("common.failed").into_owned());
            };

            let base = svc.settings.torrserver.url.as_str();
            let path = file.path.as_str();
            let file_id = file.id;
            let flag = cinebox_torrserver::StreamFlag::Play;

            let url = match cinebox_torrserver::stream_url(base, path, hash, file_id, flag) {
                Ok(url) => url,
                Err(error) => return Err(stream_error(error)),
            };

            let user = svc.settings.torrserver.username.as_str();
            let pass = svc.settings.torrserver.password.expose();
            let auth = cinebox_torrserver::basic_auth_header(user, pass);

            Ok(LoadMedia {
                url,
                headers: auth.into_iter().collect(),
                audio: None,
                proxy: None,
                start: state.time,
            })
        }
    }
}

impl PlayerScreen {
    /// Cancels an in-flight buffer wait, if any.
    ///
    /// Dropping or overwriting `self.phase` alone does not stop the
    /// background task, only an explicit `abort()` does. Call this before
    /// leaving `Buffering` so a stale wait doesn't keep polling TorrServer.
    fn abort_buffering(&mut self) {
        if let Some(PlayerPhase::Buffering(buffering)) = &mut self.phase {
            buffering.job.abort();
        }
    }

    /// The one load path: resolves the file, optionally waits for preload.
    fn begin_load(&mut self, svc: &mut Services, ctx: &egui::Context, spec: LoadSpec) {
        self.popup = Popup::None;
        self.activity.poke(ctx.input(|i| i.time));
        self.abort_buffering();
        self.skip_state.reset();

        let skip_preload = spec.source.is_youtube() || !svc.settings.torrserver.wait_preload;

        if skip_preload {
            self.phase = Some(PlayerPhase::Playing(PlayerState::from_spec(&spec)));
            self.load_current(svc);
            return;
        }

        stop_player(svc);

        let (path, file_id, resume_bytes, hash_owned) = {
            let files = spec.source.files();
            let Some(file) = files.get(spec.source.file_index()) else {
                return;
            };

            let Some(hash) = spec.source.torrent_hash() else {
                return;
            };

            let resume_bytes = resume_bytes_for_file(file, spec.source.start_seconds());

            (file.path.clone(), file.id, resume_bytes, hash.to_owned())
        };

        let meter = PreloadMeter::new();
        let mut job: Bind<(), JobError> = Bind::new(true);
        job.set_abort(true);

        let torr = jobs::TorrCtx::from(&svc.settings);
        let live = meter.clone();
        let repaint = ctx.clone();

        job.request(async move {
            jobs::wait_stream(
                torr,
                path,
                hash_owned,
                file_id,
                resume_bytes,
                move |event| {
                    live.on_event(event);
                    repaint.request_repaint();
                },
            )
            .await
        });

        let PlaySource::Torrent {
            hash,
            files,
            file_index,
            start,
        } = spec.source
        else {
            return;
        };

        self.phase = Some(PlayerPhase::Buffering(Buffering {
            card: spec.card,
            title: spec.title,
            backdrop_path: spec.backdrop_path,
            hash,
            files,
            file_index,
            resume_at: start,
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
                Some(Err(error)) => Err(UserError::from(error).summary()),
            }
        };

        let Some(PlayerPhase::Buffering(buffered)) = self.phase.take() else {
            return;
        };

        let mut state = PlayerState::from_spec(&LoadSpec {
            card: buffered.card,
            title: buffered.title,
            source: PlaySource::Torrent {
                hash: buffered.hash,
                files: buffered.files,
                file_index: buffered.file_index,
                start: buffered.resume_at,
            },
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

        let Some(player) = svc.player.clone() else {
            state.error = Some(t!("player.player_unavailable").into_owned());
            return;
        };

        let is_youtube = state.is_youtube();
        let load = match load_args(state, svc) {
            Ok(load) => load,
            Err(error) => {
                state.error = Some(error);
                return;
            }
        };

        let media = Media {
            url: &load.url,
            headers: &load.headers,
            audio_url: load.audio.as_deref(),
            http_proxy: load.proxy.as_deref(),
            start_seconds: load.start,
            loudnorm: svc.settings.player.loudnorm,
            hardware_decoding: svc.settings.player.hardware_decoding,
        };

        if let Err(error) = player.load(&media) {
            state.error = Some(error.to_string());
            return;
        }

        state.error = None;

        log_player("set_scale", player.set_scale(prefs.scale));
        log_player("set_speed", player.set_speed(prefs.speed));
        log_player("set_volume", player.set_volume(volume));
        log_player("set_subtitle_scale", player.set_subtitle_scale(sub_scale));
        log_player("set_subtitle_delay", player.set_subtitle_delay(sub_delay));

        if is_youtube {
            return;
        }

        if prefs.aid > 0 {
            log_player("select_audio", player.select_audio(prefs.aid));
        }

        if prefs.sid > 0 {
            log_player("select_subtitle", player.select_subtitle(Some(prefs.sid)));
        } else if prefs.sid == 0 {
            log_player("select_subtitle", player.select_subtitle(None));
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

            state.file_index() + 1
        };

        self.jump_to_file(svc, ctx, index);
    }

    fn prev_file(&mut self, svc: &mut Services, ctx: &egui::Context) {
        let index = {
            let Some(PlayerPhase::Playing(state)) = &self.phase else {
                return;
            };
            let Some(index) = state.file_index().checked_sub(1) else {
                return;
            };

            index
        };

        self.jump_to_file(svc, ctx, index);
    }

    fn jump_to_file(&mut self, svc: &mut Services, ctx: &egui::Context, index: usize) {
        self.save_progress(svc, true);

        let spec = {
            let Some(phase) = &self.phase else {
                return;
            };

            let (card, source, backdrop_path, current) = match phase {
                PlayerPhase::Playing(state) => {
                    if state.is_youtube() {
                        return;
                    }

                    (
                        state.card.clone(),
                        state.source.clone(),
                        state.backdrop_path.clone(),
                        Some(state.file_index()),
                    )
                }
                PlayerPhase::Buffering(state) => (
                    state.card.clone(),
                    PlaySource::Torrent {
                        hash: state.hash.clone(),
                        files: state.files.clone(),
                        file_index: state.file_index,
                        start: state.resume_at,
                    },
                    state.backdrop_path.clone(),
                    None,
                ),
            };

            if current == Some(index) {
                return;
            }

            let (title, start) = {
                let Some(file) = source.files().get(index) else {
                    return;
                };

                (file.title.clone(), file.timecode)
            };

            let PlaySource::Torrent { hash, files, .. } = source else {
                return;
            };

            LoadSpec {
                card,
                title,
                source: PlaySource::Torrent {
                    hash,
                    files,
                    file_index: index,
                    start,
                },
                backdrop_path,
            }
        };

        self.begin_load(svc, ctx, spec);
    }

    fn skip_or_next(
        &mut self,
        svc: &mut Services,
        ctx: &egui::Context,
        target: f64,
        duration: f64,
        has_next: bool,
    ) {
        let at_end = duration > 1.0 && target >= duration - 1.0;

        if at_end && has_next && svc.settings.player.auto_next {
            self.next_file(svc, ctx);
            return;
        }

        self.seek_abs(svc, target);
    }

    fn toggle(&mut self, svc: &Services) {
        let Some(PlayerPhase::Playing(state)) = &mut self.phase else {
            return;
        };

        let next = !state.paused;
        let ok = with_player(svc, |player| match player.set_paused(next) {
            Ok(()) => true,
            Err(error) => {
                warn!(%error, "pause failed");
                false
            }
        })
        .unwrap_or(false);

        if ok {
            state.paused = next;
        }
    }

    fn toggle_mute(&mut self, svc: &Services) {
        let Some(PlayerPhase::Playing(state)) = &mut self.phase else {
            return;
        };

        let next = !state.muted;
        let ok = with_player(svc, |player| match player.set_muted(next) {
            Ok(()) => true,
            Err(error) => {
                warn!(%error, "mute failed");
                false
            }
        })
        .unwrap_or(false);

        if ok {
            state.muted = next;
        }
    }

    fn seek(&self, svc: &Services, delta: f64) {
        with_player(svc, |player| {
            log_player("seek_by", player.seek_by(delta));
        });
    }

    fn seek_abs(&mut self, svc: &Services, to: f64) {
        with_player(svc, |player| {
            log_player("seek_to", player.seek_to(to));
        });

        if let Some(PlayerPhase::Playing(state)) = &mut self.phase {
            state.time = to;
        }
    }
}

fn pauses(command: MediaCommand) -> bool {
    match command {
        MediaCommand::Play | MediaCommand::Pause | MediaCommand::Stop | MediaCommand::PlayPause => {
            true
        }
        MediaCommand::SeekTo(_)
        | MediaCommand::SeekBy(_)
        | MediaCommand::Next
        | MediaCommand::Previous => false,
    }
}

/// Log a player control failure instead of dropping it silently.
fn log_player(op: &'static str, result: Result<(), cinebox_player::Error>) {
    if let Err(error) = result {
        warn!(%error, op, "player control failed");
    }
}

fn with_player<R>(svc: &Services, f: impl FnOnce(&dyn Player) -> R) -> Option<R> {
    let player = svc.player.as_deref()?;

    Some(f(player))
}

fn stop_player(svc: &Services) {
    with_player(svc, |player| player.stop());
}

fn player_tracks(svc: &Services) -> Vec<Track> {
    with_player(svc, |player| player.tracks()).unwrap_or_default()
}

fn failure_message(failure: &Failure) -> String {
    let message = match failure {
        Failure::Unsupported => t!("player.unsupported_format"),
        Failure::Network => t!("player.stream_failed"),
        Failure::Other(_) => t!("player.playback_failed"),
    };

    message.into_owned()
}

struct VideoFrame<'a> {
    scale: VideoScale,
    size: Option<[u32; 2]>,
    subtitle: Option<&'a str>,
    subtitle_scale: f64,
}

/// Returns the cached render callback, rebuilding it only when the player
/// changes; allocating a new `Arc<CallbackFn>` per frame is waste.
fn video_callback(
    cache: &mut Option<VideoCallback>,
    player: Arc<dyn Player>,
) -> Arc<egui_glow::CallbackFn> {
    let cached = cache.as_ref().filter(|cb| Arc::ptr_eq(&cb.player, &player));

    if let Some(cb) = cached {
        return cb.callback.clone();
    }

    let render_player = player.clone();
    let callback = Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
        let vp = info.viewport_in_pixels();
        let fbo = painter.intermediate_fbo().map(|fb| fb.0.get()).unwrap_or(0);
        let _ = render_player.render(fbo, vp.width_px, vp.height_px);
    }));

    *cache = Some(VideoCallback {
        player,
        callback: callback.clone(),
    });

    callback
}

fn paint_video(
    ui: &Ui,
    rect: Rect,
    player: Option<Arc<dyn Player>>,
    theme: &Theme,
    cache: &mut Option<VideoCallback>,
    frame: &VideoFrame<'_>,
) {
    let Some(player) = player else {
        ui.painter().rect_filled(rect, 0.0, theme.video_bg);
        return;
    };

    let draws_subtitles = player.features().draws_subtitles;
    match player.output() {
        VideoOutput::Rendered => {
            ui.painter().rect_filled(rect, 0.0, theme.video_bg);
            let callback = video_callback(cache, player);
            ui.painter().add(egui::PaintCallback { rect, callback });
        }
        VideoOutput::Underlay => paint_underlay(ui, rect, player.as_ref(), theme, frame),
    }

    if draws_subtitles {
        return;
    }

    if let Some(text) = frame.subtitle {
        paint_subtitle(ui, rect, text, frame.subtitle_scale);
    }
}

/// The picture shows through the window where nothing is painted, so only the
/// bars around it are filled. Until its size is known the whole rect stays dark.
fn paint_underlay(ui: &Ui, rect: Rect, player: &dyn Player, theme: &Theme, frame: &VideoFrame<'_>) {
    let pixels_per_point = ui.ctx().pixels_per_point();
    let area = to_area(rect);

    let Some(size) = frame.size else {
        ui.painter().rect_filled(rect, 0.0, theme.video_bg);
        player.place_video(area.to_pixels(pixels_per_point));
        return;
    };

    let picture = fit_video(area, size, frame.scale);
    for bar in bars_around(rect, to_rect(picture)) {
        ui.painter().rect_filled(bar, 0.0, theme.video_bg);
    }

    player.place_video(picture.to_pixels(pixels_per_point));
}

fn to_area(rect: Rect) -> Area {
    Area {
        x: rect.left(),
        y: rect.top(),
        width: rect.width(),
        height: rect.height(),
    }
}

fn to_rect(area: Area) -> Rect {
    Rect::from_min_size(pos2(area.x, area.y), egui::vec2(area.width, area.height))
}

/// The parts of `outer` that `inner` leaves uncovered, as up to four bars.
fn bars_around(outer: Rect, inner: Rect) -> Vec<Rect> {
    let inner = inner.intersect(outer);
    if !inner.is_positive() {
        return vec![outer];
    }

    let top = Rect::from_min_max(outer.min, pos2(outer.right(), inner.top()));
    let bottom = Rect::from_min_max(pos2(outer.left(), inner.bottom()), outer.max);
    let left = Rect::from_min_max(
        pos2(outer.left(), inner.top()),
        pos2(inner.left(), inner.bottom()),
    );
    let right = Rect::from_min_max(
        pos2(inner.right(), inner.top()),
        pos2(outer.right(), inner.bottom()),
    );

    [top, bottom, left, right]
        .into_iter()
        .filter(|bar| bar.is_positive())
        .collect()
}

/// Subtitle height as a share of the video rect, close to mpv's default.
const SUBTITLE_SHARE: f32 = 0.053;
/// Distance of the last line from the rect's bottom edge, as a share of its height.
const SUBTITLE_MARGIN_SHARE: f32 = 0.05;
/// Outline width in points, drawn as offset copies (egui text has no stroke).
const SUBTITLE_OUTLINE: f32 = 2.0;

fn paint_subtitle(ui: &Ui, rect: Rect, text: &str, scale: f64) {
    let size = rect.height() * SUBTITLE_SHARE * scale as f32;
    let font = egui::FontId::proportional(size);
    let wrap = rect.width() * 0.9;

    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, Color32::WHITE, wrap);
    job.halign = Align::Center;
    let galley = ui.painter().layout_job(job);

    let inset = crate::platform::edge_inset(ui.ctx());
    let margin = (rect.height() * SUBTITLE_MARGIN_SHARE).max(f32::from(inset.bottom));
    let top = rect.bottom() - margin - galley.size().y;
    let anchor = pos2(rect.center().x, top);

    let painter = ui.painter();
    for dx in [-SUBTITLE_OUTLINE, 0.0, SUBTITLE_OUTLINE] {
        for dy in [-SUBTITLE_OUTLINE, 0.0, SUBTITLE_OUTLINE] {
            let offset = egui::vec2(dx, dy);
            painter.galley_with_override_text_color(
                anchor + offset,
                galley.clone(),
                Color32::BLACK,
            );
        }
    }
    painter.galley(anchor, galley, Color32::WHITE);
}

fn stream_error(error: cinebox_torrserver::Error) -> String {
    let error = JobError::from(error);
    UserError::from(&error).summary()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::*;

    /// `Bind::request` spawns onto `egui_async`'s own runtime, independent of
    /// the UI ever polling it again, so dropping the `Bind` alone does not
    /// stop the task. This is the mechanism behind the "leaves the player
    /// while buffering" question: `PlayerScreen::stop` used to just drop
    /// `self.phase`.
    #[test]
    fn dropping_bind_leaves_the_spawned_task_running() {
        let completed = Arc::new(AtomicBool::new(false));
        let flag = completed.clone();

        let mut job: Bind<(), String> = Bind::new(true);
        job.set_abort(true);
        job.request(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            flag.store(true, Ordering::SeqCst);
            Ok(())
        });

        drop(job);
        std::thread::sleep(Duration::from_millis(400));

        assert!(
            completed.load(Ordering::SeqCst),
            "task should have run to completion: dropping Bind does not cancel it"
        );
    }

    /// The fix: calling `abort()` before the `Bind` is dropped/replaced does
    /// physically cancel the task (requires `set_abort(true)`, set in `begin_load`).
    #[test]
    fn explicit_abort_before_drop_cancels_the_task() {
        let completed = Arc::new(AtomicBool::new(false));
        let flag = completed.clone();

        let mut job: Bind<(), String> = Bind::new(true);
        job.set_abort(true);
        job.request(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            flag.store(true, Ordering::SeqCst);
            Ok(())
        });

        job.abort();
        drop(job);
        std::thread::sleep(Duration::from_millis(400));

        assert!(
            !completed.load(Ordering::SeqCst),
            "task should have been cancelled by abort()"
        );
    }

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

    pub(super) fn spec(file_index: usize, count: i32) -> LoadSpec {
        LoadSpec {
            card: WatchCard {
                kind: cinebox_core::MediaKind::Tv,
                id: cinebox_core::TmdbId::new(1),
                section: cinebox_core::Section::Tv,
                title: String::from("Show"),
                poster_path: None,
                year: Some(2024),
                vote: Some(8.0),
            },
            title: String::from("Episode"),
            source: PlaySource::Torrent {
                hash: String::from("deadbeef"),
                files: (1..=count).map(file).collect(),
                file_index,
                start: 12.0,
            },
            backdrop_path: None,
        }
    }

    pub(super) fn youtube_spec() -> LoadSpec {
        LoadSpec {
            card: WatchCard {
                kind: cinebox_core::MediaKind::Movie,
                id: cinebox_core::TmdbId::new(1),
                section: cinebox_core::Section::Movies,
                title: String::from("Movie"),
                poster_path: None,
                year: Some(2024),
                vote: Some(8.0),
            },
            title: String::from("Trailer"),
            source: PlaySource::Youtube {
                video_url: String::from("https://example.test/v"),
                audio_url: None,
                http_header_fields: Vec::new(),
            },
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
    fn youtube_spec_skips_playlist_and_resume() {
        let state = PlayerState::from_spec(&youtube_spec());

        assert!(state.is_youtube());
        assert!(!state.has_next());
        assert!(state.files().is_empty());
        assert!(state.time.abs() < f64::EPSILON);
    }

    #[test]
    fn http_stream_started_ignores_zero_duration_if_clock_moved() {
        assert!(!http_stream_started(0.0, 0.0));
        assert!(http_stream_started(0.4, 0.0));
        assert!(http_stream_started(0.0, 90.0));
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
}
