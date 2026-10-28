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
use cinebox_player::{Area, ClickZone, Failure, Media, Player, SEEK_SECS, Track, VideoOutput, click_zone, fit_video};
use cinebox_skip::SegmentType;
use egui::{Align, Align2, Color32, CursorIcon, Rect, RichText, Sense, Ui, pos2};
use egui_async::Bind;
use rust_i18n::t;
use tracing::{info, trace, warn};

use self::buffering::{Buffering, PreloadMeter};
use self::overlay::{Activity, FooterOut, FooterView};
use crate::app::nav::NavAction;
use crate::i18n::errors::UserError;
use crate::platform::MediaCommand;
use crate::screens::play::{PlayRequest, PlaySource, WatchCard};
use crate::screens::torrents::TorrentFileRow;
use crate::services::jobs::{self, JobError};
use crate::services::{Services, db_block_on};
use crate::theme::{self, Theme};
use crate::widgets::flyout;

#[expect(
    clippy::struct_excessive_bools,
    reason = "the player's own flags, mirrored each frame, beside the tail tracking"
)]
struct PlayerState {
    card: WatchCard,
    title: String,
    source: PlaySource,
    backdrop_path: Option<String>,
    is_paused: bool,
    time: f64,
    duration: f64,
    error: Option<String>,
    is_muted: bool,
    volume: f64,
    video_size: Option<[u32; 2]>,
    /// Drawn by the app when the player leaves subtitles to it.
    subtitle: Option<String>,
    loaded_at: Instant,
    decoder: Option<cinebox_player::VideoDecoder>,
    /// The start point was compared with the duration (see [`Self::track_tail`]).
    is_start_checked: bool,
    /// Playback was seen before the file's tail since it loaded; only then
    /// does its end move on to the next file.
    has_played_before_tail: bool,
}

impl PlayerState {
    fn from_spec(spec: &LoadSpec) -> Self {
        Self {
            card: spec.card.clone(),
            title: spec.title.clone(),
            source: spec.source.clone(),
            backdrop_path: spec.backdrop_path.clone(),
            is_paused: false,
            time: spec.source.start_seconds(),
            duration: 0.0,
            error: None,
            is_muted: false,
            volume: 100.0,
            video_size: None,
            subtitle: None,
            loaded_at: Instant::now(),
            decoder: None,
            is_start_checked: false,
            has_played_before_tail: false,
        }
    }

    /// Feeds a position once the duration is known. `true` when the file was
    /// opened at its very end, watched before: it starts over rather than
    /// ending at once and handing over to the next file.
    fn track_tail(&mut self, time: f64, duration: f64) -> bool {
        let tail = duration - FINISHED_TAIL_SECS.min(duration / 10.0);
        if time < tail {
            self.has_played_before_tail = true;
        }

        if self.is_start_checked {
            return false;
        }

        self.is_start_checked = true;
        self.source.start_seconds() >= tail
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
pub(crate) struct Played {
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

#[expect(clippy::struct_excessive_bools, reason = "independent screen flags, each set and cleared on its own")]
pub(crate) struct PlayerScreen {
    phase: Option<PlayerPhase>,
    is_fullscreen: bool,
    /// The window was maximized before fullscreen and is maximized again after it.
    was_maximized: bool,
    activity: Activity,
    /// TV: the D-pad was on the transport controls, so Back hides them first.
    has_controls_focus: bool,
    /// Play/pause takes the focus once the footer is drawn.
    is_play_focus_pending: bool,
    popup: Popup,
    /// See [`settings_popup::View::left`].
    settings_left: Option<settings_popup::Page>,
    /// The playlist scrolls to the current file once it opens.
    is_playlist_scroll_pending: bool,
    prefs: TorrentPlaybackPrefs,
    sub_scale: f64,
    sub_delay: f64,
    /// The volume changed since the settings were last saved.
    is_volume_dirty: bool,
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
            is_fullscreen: false,
            was_maximized: false,
            activity: Activity::new(),
            has_controls_focus: false,
            is_play_focus_pending: false,
            popup: Popup::None,
            settings_left: None,
            is_playlist_scroll_pending: false,
            prefs: TorrentPlaybackPrefs::default(),
            sub_scale: 1.0,
            sub_delay: 0.0,
            is_volume_dirty: false,
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
    pub(crate) fn start(&mut self, req: PlayRequest, svc: &mut Services, ctx: &egui::Context) {
        self.prefs = TorrentPlaybackPrefs::default();

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
            LoadSpec { card: req.card, title: req.title, source: req.source, backdrop_path: req.backdrop_path },
        );
    }

    /// Saves progress and stops. Returns the source it stopped on, with the
    /// timecodes just saved.
    pub(crate) fn stop(&mut self, svc: &mut Services, ctx: &egui::Context) -> Option<Played> {
        let saved = self.save_progress(svc, true);
        stop_player(svc);

        self.abort_buffering();
        self.skip_state.reset();
        let played = self.phase.take().map(|phase| Played { source: phase.into_source(), saved });
        self.popup = Popup::None;

        if self.is_volume_dirty {
            svc.persist();
            self.is_volume_dirty = false;
        }

        self.set_fullscreen(ctx, false);
        crate::platform::device(ctx).keep_screen_on(false);
        self.session.clear(ctx);

        played
    }

    /// Pause without toggling (the app went to the background).
    pub(crate) fn pause(&mut self, svc: &Services) {
        let is_playing = matches!(&self.phase, Some(PlayerPhase::Playing(state)) if !state.is_paused);
        if is_playing {
            self.toggle(svc);
        }
    }

    fn resume(&mut self, svc: &Services) {
        let is_paused = matches!(&self.phase, Some(PlayerPhase::Playing(state)) if state.is_paused);
        if is_paused {
            self.toggle(svc);
        }
    }

    /// Transport control from outside the player UI.
    pub(crate) fn apply(&mut self, command: MediaCommand, svc: &mut Services, ctx: &egui::Context) {
        match command {
            MediaCommand::Play => self.resume(svc),
            MediaCommand::Pause | MediaCommand::Stop => self.pause(svc),
            MediaCommand::PlayPause => self.toggle(svc),
            MediaCommand::SeekTo(secs) => self.seek_abs(svc, secs),
            MediaCommand::SeekBy(secs) => seek_by(svc, secs),
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
            self.is_play_focus_pending = true;
        }
    }

    /// Video with nothing of the app's chrome around it. Where the OS owns the
    /// window there is no fullscreen to toggle: the player always fills it.
    #[must_use]
    pub(crate) fn fills_screen(&self, profile: crate::platform::Profile) -> bool {
        if !profile.is_desktop_window() {
            return true;
        }

        self.is_fullscreen
    }

    pub(crate) fn tick(&mut self, svc: &mut Services, ctx: &egui::Context) {
        self.sync_fullscreen(ctx);
        self.poll_buffering(svc);

        let Some(PlayerPhase::Playing(state)) = &mut self.phase else { return };

        let Some(player) = &svc.player else { return };

        let snapshot = player.snapshot();
        // Logged on every change: the player may fall back to software mid-stream.
        let decoder = player.video_decoder();
        if decoder.is_some() && decoder != state.decoder {
            if let Some(cinebox_player::VideoDecoder { hwdec, codec, pixel_format }) = &decoder {
                info!(%hwdec, %codec, %pixel_format, "video decoder");
            }

            state.decoder = decoder;
        }

        state.is_paused = snapshot.paused;
        state.time = snapshot.time;
        state.duration = snapshot.duration;
        state.is_muted = snapshot.muted;
        state.volume = snapshot.volume;
        state.video_size = snapshot.video_size;
        state.subtitle = snapshot.subtitle;

        if let Some(failure) = &snapshot.failure
            && state.error.is_none()
        {
            warn!(?failure, "playback failed");
            state.error = Some(failure_message(failure));
        }

        let has_started = http_stream_started(snapshot.time, snapshot.duration);
        let is_waiting = state.is_youtube() && state.error.is_none() && !has_started;
        let is_stalled = is_waiting && state.loaded_at.elapsed() > STREAM_STALL;

        if is_stalled {
            warn!("http stream did not start");
            state.error = Some(t!("player.stream_stalled").into_owned());
        }

        let is_restart_due = snapshot.duration > 1.0 && state.track_tail(snapshot.time, snapshot.duration);
        let is_finished = snapshot.eof && state.has_played_before_tail;
        let is_next_due = is_finished && svc.settings.player.auto_next && state.has_next();

        if is_stalled {
            stop_player(svc);
        }

        if is_restart_due {
            self.seek_abs(svc, 0.0);
        }

        if let Some(Err(error)) = self.viewed_job.take() {
            warn!(%error, "failed to mark the file viewed in TorrServer");
        }

        self.save_progress(svc, false);

        if is_next_due {
            self.next_file(svc, ctx);
        }

        // Read again: a jump to the next file above replaced the state.
        if let Some(PlayerPhase::Playing(playing)) = &self.phase {
            self.session.publish(svc, playing, ctx);
        }
    }

    pub(crate) fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
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
                self.playing_ui(ui, svc, theme, video);
                None
            }
        }
    }
}

/// What one frame of playback draws, copied out of [`PlayerState`].
struct PlayingView {
    title: String,
    error: Option<String>,
    time: f64,
    duration: f64,
    is_paused: bool,
    is_muted: bool,
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

impl From<&PlayerState> for PlayingView {
    fn from(state: &PlayerState) -> Self {
        let current = state.files().get(state.file_index());

        Self {
            title: state.title.clone(),
            error: state.error.clone(),
            time: state.time,
            duration: state.duration,
            is_paused: state.is_paused,
            is_muted: state.is_muted,
            volume: state.volume,
            video_size: state.video_size,
            subtitle: state.subtitle.clone(),
            file_count: state.files().len(),
            file_index: state.file_index(),
            has_next: state.has_next(),
            kind: state.card.kind,
            tmdb_id: state.card.id,
            season: current.and_then(|file| file.season),
            episode: current.and_then(|file| file.episode),
        }
    }
}

impl PlayingView {
    fn skip_target(&self) -> skip::SkipTarget {
        skip::SkipTarget { kind: self.kind, tmdb_id: self.tmdb_id, season: self.season, episode: self.episode }
    }

    fn playhead(&self) -> skip::Playhead {
        skip::Playhead { time_secs: self.time, duration_secs: self.duration, is_paused: self.is_paused }
    }
}

/// What a click on the video asks for.
#[derive(Debug, Default)]
struct VideoClick {
    seek_by: Option<f64>,
    is_pause_toggled: bool,
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

    fn playing_ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme, video: Rect) {
        let Some(PlayerPhase::Playing(state)) = &self.phase else { return };

        let view = PlayingView::from(state);
        let now = ui.input(|i| i.time);
        let ctx = ui.ctx().clone();
        let was_popup_open = self.popup != Popup::None;

        let (rect, _) = ui.allocate_exact_size(video.size(), Sense::hover());
        let response = ui.interact(rect, input::video_id(), Sense::click());
        if self.dpad_video_focus(ui, &response, was_popup_open, now) {
            self.is_play_focus_pending = true;
        }

        if let Some(error) = view.error.as_deref() {
            ui.painter().rect_filled(rect, 0.0, theme.video_bg);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, error, theme.ui_font(theme.text_body), theme.err);
        } else {
            let frame = VideoFrame {
                scale: self.prefs.scale,
                size: view.video_size,
                subtitle: view.subtitle.as_deref(),
                subtitle_scale: self.sub_scale,
                background: theme.video_bg,
            };
            paint_video(ui, rect, svc.player.clone(), &mut self.video_cb, &frame);
        }

        let is_video_clicked = view.error.is_none() && !was_popup_open && response.clicked();
        let click = if is_video_clicked { self.video_click(&ctx, &response, rect) } else { VideoClick::default() };

        if was_popup_open {
            self.activity.poke(now);
        }

        let alpha = self.activity.visual_t(now);
        overlay::header(&ctx, theme, rect, &view.title, alpha);

        let footer_view = self.footer_view(&view, svc, &ctx);
        let footer = overlay::footer(&ctx, theme, rect, &footer_view, alpha);
        if std::mem::take(&mut self.is_play_focus_pending) {
            ctx.memory_mut(|mem| mem.request_focus(footer.play_id));
        }

        let focused = ctx.memory(egui::Memory::focused);
        self.has_controls_focus = focused.is_some_and(|id| id != input::video_id());

        self.popups(&ctx, svc, theme, &footer);
        self.tick_skip(svc, &ctx, &view, now);

        let banner = skip_overlay::show(&self.skip_state, &ctx, theme, rect, footer.seek_rect);
        self.answer_skip_banner(svc, &ctx, &view, &banner);
        self.apply_footer(svc, &ctx, &footer, &click);

        // Last, so that it wins over the pointing hand of the clickable video.
        hide_resting_cursor(&ctx, rect, alpha);
        repaint_chrome(&ctx, alpha);
    }

    /// What a click on the video asks for, by where it landed.
    fn video_click(&mut self, ctx: &egui::Context, response: &egui::Response, rect: Rect) -> VideoClick {
        let Some(pos) = response.interact_pointer_pos() else {
            // OK on the remote: no pointer, so no click zone.
            self.focus_play_from_remote(ctx);
            return VideoClick { is_pause_toggled: true, ..VideoClick::default() };
        };

        let x_ratio = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);

        match click_zone(x_ratio) {
            ClickZone::SeekBack => VideoClick { seek_by: Some(-SEEK_SECS), ..VideoClick::default() },
            ClickZone::Pause => VideoClick { is_pause_toggled: true, ..VideoClick::default() },
            ClickZone::SeekFwd => VideoClick { seek_by: Some(SEEK_SECS), ..VideoClick::default() },
        }
    }

    fn footer_view(&self, view: &PlayingView, svc: &Services, ctx: &egui::Context) -> FooterView {
        let profile = crate::platform::profile(ctx);

        FooterView {
            time: view.time,
            duration: view.duration,
            is_paused: view.is_paused,
            is_muted: view.is_muted,
            volume: view.volume,
            has_volume_control: svc.player_features().volume,
            file_count: view.file_count,
            file_index: view.file_index,
            has_next: view.has_next,
            is_fullscreen: self.is_fullscreen,
            can_fullscreen: profile.is_desktop_window(),
            is_directional: profile.is_directional(),
        }
    }

    fn tick_skip(&mut self, svc: &mut Services, ctx: &egui::Context, view: &PlayingView, now: f64) {
        if let Some(Err(error)) = self.skip_save_job.take() {
            warn!(%error, "failed to save the skip choice");
        }

        if let Some(target) = self.skip_state.tick(now, view.playhead(), view.skip_target(), svc) {
            self.skip_or_next(svc, ctx, target, view);
        }
    }

    fn answer_skip_banner(
        &mut self,
        svc: &mut Services,
        ctx: &egui::Context,
        view: &PlayingView,
        banner: &skip_overlay::SkipBannerOut,
    ) {
        if banner.is_skip_clicked
            && let Some((target, ty)) = self.skip_state.on_skip(view.duration)
        {
            self.skip_or_next(svc, ctx, target, view);
            self.save_skip_choice(svc, view, ty, true);
        }

        if banner.is_cancel_clicked
            && let Some(ty) = self.skip_state.on_cancel()
        {
            self.save_skip_choice(svc, view, ty, false);
        }
    }

    fn save_skip_choice(&mut self, svc: &Services, view: &PlayingView, ty: SegmentType, is_armed: bool) {
        let Some(db) = svc.db.clone() else { return };

        let job = jobs::save_skip_choice(db, view.kind, view.tmdb_id, ty, is_armed);
        self.skip_save_job.request(job);
    }

    fn apply_footer(&mut self, svc: &mut Services, ctx: &egui::Context, footer: &FooterOut, click: &VideoClick) {
        if let Some(to) = footer.seek_to {
            self.seek_abs(svc, to);
        }

        if let Some(delta) = footer.seek_rel.or(click.seek_by) {
            seek_by(svc, delta);
        }

        if footer.is_pause_toggled || click.is_pause_toggled {
            self.toggle(svc);
        }

        if footer.is_prev_clicked {
            self.prev_file(svc, ctx);
        }

        if footer.is_next_clicked {
            self.next_file(svc, ctx);
        }

        if footer.is_playlist_clicked {
            self.toggle_popup(Popup::Playlist);
        }

        if footer.is_settings_clicked {
            self.toggle_popup(Popup::Settings(settings_popup::Page::Root));
        }

        if footer.is_volume_clicked {
            self.toggle_mute(svc);
        }

        if footer.is_fullscreen_clicked {
            let next = !self.is_fullscreen;
            self.set_fullscreen(ctx, next);
        }

        if footer.is_volume_hovered && self.popup == Popup::None {
            self.popup = Popup::Volume;
        }
    }

    fn popups(&mut self, ctx: &egui::Context, svc: &mut Services, theme: &Theme, footer: &FooterOut) {
        match self.popup {
            Popup::None => {}
            Popup::Settings(_) => self.settings_popup(ctx, svc, theme, footer.settings_rect),
            Popup::Playlist => self.playlist_popup(ctx, svc, theme, footer.playlist_rect),
            Popup::Volume => self.volume_popup(ctx, svc, theme, footer.volume_rect),
        }
    }

    fn settings_popup(&mut self, ctx: &egui::Context, svc: &mut Services, theme: &Theme, anchor: Rect) {
        let Popup::Settings(page) = self.popup else {
            return;
        };

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
        let placement = flyout::Placement { id: "player-settings-flyout", anchor, width: 280.0 };
        let fly = flyout::show(ctx, theme, placement, |ui, theme| {
            out = settings_popup::paint(ui, theme, &view);
        });

        if let Some(next) = out.page {
            self.turn_settings_page(page, next);
        }

        let mut is_dirty = false;

        if let Some(scale) = out.scale {
            self.prefs.scale = scale;
            with_player(svc, |player| {
                log_player("set_scale", player.set_scale(scale));
            });
            is_dirty = true;
        }

        if let Some(speed) = out.speed {
            self.prefs.speed = speed;
            with_player(svc, |player| {
                log_player("set_speed", player.set_speed(speed));
            });
            is_dirty = true;
        }

        if let Some(id) = out.audio {
            self.prefs.aid = id;
            with_player(svc, |player| {
                log_player("select_audio", player.select_audio(id));
            });
            is_dirty = true;
        }

        if let Some(pick) = out.sub {
            let track = pick.track_id();
            self.prefs.sid = track.unwrap_or(0);
            with_player(svc, |player| {
                log_player("select_subtitle", player.select_subtitle(track));
            });
            is_dirty = true;
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

        if is_dirty {
            self.save_prefs(svc);
        }

        if fly.dismissed {
            self.popup = Popup::None;
        }
    }

    fn playlist_popup(&mut self, ctx: &egui::Context, svc: &mut Services, theme: &Theme, anchor: Rect) {
        let mut jump = None;
        let mut scroll_to_current = self.is_playlist_scroll_pending;

        let dismissed = {
            let Some(PlayerPhase::Playing(state)) = &self.phase else {
                return;
            };
            let playlist = playlist_popup::Playlist { files: state.files(), current: state.file_index() };

            let placement = flyout::Placement { id: "player-playlist-flyout", anchor, width: 380.0 };
            let fly = flyout::show(ctx, theme, placement, |ui, theme| {
                jump = playlist_popup::paint(ui, theme, svc, playlist, &mut scroll_to_current);
            });

            fly.dismissed
        };

        self.is_playlist_scroll_pending = scroll_to_current;

        if let Some(index) = jump {
            self.popup = Popup::None;
            self.jump_to_file(svc, ctx, index);
            return;
        }

        if dismissed {
            self.popup = Popup::None;
        }
    }

    fn volume_popup(&mut self, ctx: &egui::Context, svc: &mut Services, theme: &Theme, anchor: Rect) {
        let mut level = svc.settings.player.volume;
        let mut has_changed = false;

        let placement = flyout::Placement { id: "player-volume-flyout", anchor, width: volume::WIDTH };
        let fly = flyout::show(ctx, theme, placement, |ui, theme| {
            has_changed = volume::slider(ui, theme, &mut level);
        });

        if has_changed {
            let level = level.clamp(0.0, 100.0);
            svc.settings.player.volume = level;
            with_player(svc, |player| {
                log_player("set_volume", player.set_volume(level));
            });
            self.is_volume_dirty = true;
        }

        let pointer = ctx.pointer_latest_pos();
        let is_over = pointer.is_some_and(|pos| fly.rect.expand(8.0).contains(pos) || anchor.expand(8.0).contains(pos));

        if is_over {
            return;
        }

        self.popup = Popup::None;

        if self.is_volume_dirty {
            svc.persist();
            self.is_volume_dirty = false;
        }
    }

    fn toggle_popup(&mut self, popup: Popup) {
        let is_same = std::mem::discriminant(&self.popup) == std::mem::discriminant(&popup);
        if is_same {
            self.popup = Popup::None;
            return;
        }

        if matches!(popup, Popup::Playlist) {
            self.is_playlist_scroll_pending = true;
        }

        self.settings_left = None;
        self.popup = popup;
    }

    fn turn_settings_page(&mut self, from: settings_popup::Page, to: settings_popup::Page) {
        self.settings_left = Some(from);
        self.popup = Popup::Settings(to);
    }
}

/// A start point this close to the end means the file was watched to it.
const FINISHED_TAIL_SECS: f64 = 30.0;

/// Resumes this close to the start keep the stock head preload: its window
/// usually reaches the seek target, and a dedicated mid-file wait isn't worth it.
const RESUME_PRELOAD_MIN_SECS: f64 = 60.0;
const STREAM_STALL: Duration = Duration::from_secs(20);
/// How often the shown chrome repaints to keep its clock moving.
const CHROME_TICK: Duration = Duration::from_millis(120);

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
    let share = (resume_at / duration).clamp(0.0, 0.99);

    Some(share_of_length(file.length, share))
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "an estimate: `share` is below 1, so the offset stays inside the file, which `f64` measures closely enough"
)]
fn share_of_length(length: u64, share: f64) -> u64 {
    (share * length as f64) as u64
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
        PlaySource::Youtube { video_url, audio_url, http_header_fields } => {
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
        PlaySource::Torrent { hash, files, file_index, .. } => {
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

            Ok(LoadMedia { url, headers: auth.into_iter().collect(), audio: None, proxy: None, start: state.time })
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

        let can_skip_preload = spec.source.is_youtube() || !svc.settings.torrserver.wait_preload;

        if can_skip_preload {
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
            let file = jobs::StreamFile { path, hash: hash_owned, id: file_id, resume_bytes };

            jobs::wait_stream(torr, file, move |event| {
                live.on_event(event);
                repaint.request_repaint();
            })
            .await
        });

        let PlaySource::Torrent { hash, files, file_index, start } = spec.source else {
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

        let has_failed = state.error.is_some();
        self.phase = Some(PlayerPhase::Playing(state));

        if !has_failed {
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

                    (state.card.clone(), state.source.clone(), state.backdrop_path.clone(), Some(state.file_index()))
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
                source: PlaySource::Torrent { hash, files, file_index: index, start },
                backdrop_path,
            }
        };

        self.begin_load(svc, ctx, spec);
    }

    /// Seeks to `target`, or moves on to the next file when that is the end of this one.
    fn skip_or_next(&mut self, svc: &mut Services, ctx: &egui::Context, target: f64, view: &PlayingView) {
        let is_at_end = view.duration > 1.0 && target >= view.duration - 1.0;

        if is_at_end && view.has_next && svc.settings.player.auto_next {
            self.next_file(svc, ctx);
            return;
        }

        self.seek_abs(svc, target);
    }

    fn toggle(&mut self, svc: &Services) {
        let Some(PlayerPhase::Playing(state)) = &mut self.phase else { return };

        let next = !state.is_paused;
        let is_applied = with_player(svc, |player| match player.set_paused(next) {
            Ok(()) => true,
            Err(error) => {
                warn!(%error, "pause failed");
                false
            }
        });

        if is_applied == Some(true) {
            state.is_paused = next;
        }
    }

    fn toggle_mute(&mut self, svc: &Services) {
        let Some(PlayerPhase::Playing(state)) = &mut self.phase else { return };

        let next = !state.is_muted;
        let is_applied = with_player(svc, |player| match player.set_muted(next) {
            Ok(()) => true,
            Err(error) => {
                warn!(%error, "mute failed");
                false
            }
        });

        if is_applied == Some(true) {
            state.is_muted = next;
        }
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
        MediaCommand::Play | MediaCommand::Pause | MediaCommand::Stop | MediaCommand::PlayPause => true,
        MediaCommand::SeekTo(_) | MediaCommand::SeekBy(_) | MediaCommand::Next | MediaCommand::Previous => false,
    }
}

/// Log a player control failure instead of dropping it silently.
fn log_player(op: &'static str, result: Result<(), cinebox_player::Error>) {
    if let Err(error) = result {
        warn!(%error, op, "player control failed");
    }
}

fn with_player<R>(svc: &Services, control: impl FnOnce(&dyn Player) -> R) -> Option<R> {
    let player = svc.player.as_deref()?;

    Some(control(player))
}

fn stop_player(svc: &Services) {
    with_player(svc, |player| player.stop());
}

fn seek_by(svc: &Services, delta: f64) {
    with_player(svc, |player| {
        log_player("seek_by", player.seek_by(delta));
    });
}

fn hide_resting_cursor(ctx: &egui::Context, video: Rect, alpha: f32) {
    let rests_on_video = ctx.pointer_hover_pos().is_some_and(|pos| video.contains(pos));

    if alpha <= 0.0 && rests_on_video && crate::platform::profile(ctx).is_desktop_window() {
        ctx.set_cursor_icon(CursorIcon::None);
    }
}

/// Every frame while the chrome fades, then often enough for its clock while it shows.
fn repaint_chrome(ctx: &egui::Context, alpha: f32) {
    if alpha <= 0.0 {
        return;
    }

    if alpha < 1.0 {
        ctx.request_repaint();
        return;
    }

    ctx.request_repaint_after(CHROME_TICK);
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
    /// Fills the rect wherever the picture does not.
    background: Color32,
}

/// Returns the cached render callback, rebuilding it only when the player
/// changes; allocating a new `Arc<CallbackFn>` per frame is waste.
fn video_callback(cache: &mut Option<VideoCallback>, player: Arc<dyn Player>) -> Arc<egui_glow::CallbackFn> {
    let cached = cache.as_ref().filter(|cached| Arc::ptr_eq(&cached.player, &player));

    if let Some(cached) = cached {
        return Arc::clone(&cached.callback);
    }

    let render_player = Arc::clone(&player);
    let callback = Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
        let viewport = info.viewport_in_pixels();
        let fbo = painter.intermediate_fbo().map_or(0, |framebuffer| framebuffer.0.get());

        // Trace only: a failing frame fails again on every repaint, and the next one retries anyway.
        if let Err(error) = render_player.render(fbo, viewport.width_px, viewport.height_px) {
            trace!(%error, "video frame failed to render");
        }
    }));

    *cache = Some(VideoCallback { player, callback: Arc::clone(&callback) });

    callback
}

fn paint_video(
    ui: &Ui,
    rect: Rect,
    player: Option<Arc<dyn Player>>,
    cache: &mut Option<VideoCallback>,
    frame: &VideoFrame<'_>,
) {
    let Some(player) = player else {
        ui.painter().rect_filled(rect, 0.0, frame.background);
        return;
    };

    let draws_subtitles = player.features().draws_subtitles;
    match player.output() {
        VideoOutput::Rendered => {
            ui.painter().rect_filled(rect, 0.0, frame.background);
            let callback = video_callback(cache, player);
            ui.painter().add(egui::PaintCallback { rect, callback });
        }
        VideoOutput::Underlay => paint_underlay(ui, rect, player.as_ref(), frame),
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
fn paint_underlay(ui: &Ui, rect: Rect, player: &dyn Player, frame: &VideoFrame<'_>) {
    let pixels_per_point = ui.ctx().pixels_per_point();
    let area = to_area(rect);

    let Some(size) = frame.size else {
        ui.painter().rect_filled(rect, 0.0, frame.background);
        player.place_video(area.to_pixels(pixels_per_point));
        return;
    };

    let picture = fit_video(area, size, frame.scale);
    for bar in bars_around(rect, to_rect(picture)) {
        ui.painter().rect_filled(bar, 0.0, frame.background);
    }

    player.place_video(picture.to_pixels(pixels_per_point));
}

fn to_area(rect: Rect) -> Area {
    Area { x: rect.left(), y: rect.top(), width: rect.width(), height: rect.height() }
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
    let left = Rect::from_min_max(pos2(outer.left(), inner.top()), pos2(inner.left(), inner.bottom()));
    let right = Rect::from_min_max(pos2(inner.right(), inner.top()), pos2(outer.right(), inner.bottom()));

    [top, bottom, left, right].into_iter().filter(Rect::is_positive).collect()
}

/// Subtitle height as a share of the video rect, close to mpv's default.
const SUBTITLE_SHARE: f32 = 0.053;
/// Distance of the last line from the rect's bottom edge, as a share of its height.
const SUBTITLE_MARGIN_SHARE: f32 = 0.05;
/// Outline width in points, drawn as offset copies (egui text has no stroke).
const SUBTITLE_OUTLINE: f32 = 2.0;

fn paint_subtitle(ui: &Ui, rect: Rect, text: &str, scale: f64) {
    let size = rect.height() * SUBTITLE_SHARE * theme::narrow(scale);
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
            painter.galley_with_override_text_color(anchor + offset, galley.clone(), Color32::BLACK);
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

        assert!(!completed.load(Ordering::SeqCst), "task should have been cancelled by abort()");
    }

    fn file(id: i32) -> TorrentFileRow {
        let number = u32::try_from(id).unwrap();

        TorrentFileRow {
            id,
            path: format!("Season 1/E{id:02}.mkv"),
            length: 1000,
            timecode: 0.0,
            number,
            season: Some(1),
            episode: Some(number),
            title: format!("Episode {id}"),
            still_url: None,
            runtime_minutes: Some(40),
            air_date: None,
        }
    }

    pub(super) fn spec(file_index: usize, count: i32) -> LoadSpec {
        LoadSpec {
            card: WatchCard {
                kind: MediaKind::Tv,
                id: TmdbId::new(1),
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
                kind: MediaKind::Movie,
                id: TmdbId::new(1),
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

    fn state_starting_at(at: f64) -> PlayerState {
        let mut spec = spec(0, 3);
        if let PlaySource::Torrent { start, .. } = &mut spec.source {
            *start = at;
        }

        PlayerState::from_spec(&spec)
    }

    #[test]
    fn a_file_opened_at_its_end_starts_over_once() {
        let mut state = state_starting_at(2690.0);

        assert!(state.track_tail(2690.0, 2700.0));
        assert!(!state.has_played_before_tail, "its end must not hand over to the next file");

        assert!(!state.track_tail(2700.0, 2700.0));
        assert!(!state.track_tail(0.0, 2700.0));
        assert!(state.has_played_before_tail);
    }

    #[test]
    fn a_resume_mid_file_plays_on() {
        let mut state = state_starting_at(1200.0);

        assert!(!state.track_tail(1200.0, 2700.0));
        assert!(state.has_played_before_tail);
    }

    #[test]
    fn a_short_file_from_its_start_plays_on() {
        let mut state = state_starting_at(0.0);

        assert!(!state.track_tail(0.0, 20.0));
        assert!(state.has_played_before_tail);
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
