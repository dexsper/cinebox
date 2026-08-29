use std::collections::HashMap;
use std::time::{Duration, Instant};

use cinebox_core::i18n::Msg;
use cinebox_core::{
    HomeCatalog, MediaDetails, MediaKind, PersonDetails, PosterSize, Settings, SettingsStore,
    TmdbId, tmdb_image_url,
};
use iced::animation::Easing;
use iced::event::{self, Event as IcedEvent, Status as EventStatus};
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key};
use iced::window::{self, Window};
use iced::{Animation, Element, Subscription, Task};
use tracing::{error, info, warn};

use crate::images::{
    insert_image, insert_poster, queue_media_assets, queue_person_assets, queue_plain_urls,
    queue_posters,
};
use crate::loaders::{
    load_home_task, load_media_task, load_person_task, load_torrents_task, open_magnet_task,
    wait_stream_task,
};
use crate::nav::{Nav, Screen};
use crate::ui;
use crate::ui::card::MediaState;
use crate::ui::home::{ExtraImages, HomeState, PosterMap};
use crate::ui::person::PersonState;
use crate::ui::player::{self, Event as PlayerEvent, PlayerState};
use crate::ui::scroll::{self, ScrollFlash};
use crate::ui::settings::Probes;
use crate::ui::torrents::{Event as TorrentEvent, FilesPane, TorrentHits, TorrentState};

pub use crate::message::Message;

struct TmdbView {
    api_key: String,
    language: Option<String>,
    poster_size: PosterSize,
    use_system_proxy: bool,
}

enum TmdbChange {
    None,
    Catalog,
    PosterSize,
}

impl TmdbView {
    fn from_settings(settings: &Settings) -> Self {
        Self {
            api_key: settings.tmdb.api_key.expose().to_owned(),
            language: settings.tmdb.data_language.clone(),
            poster_size: settings.tmdb.poster_size,
            use_system_proxy: settings.interface.use_system_proxy,
        }
    }

    fn has_key(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn change_from(&self, next: &Self) -> TmdbChange {
        let key_changed = next.api_key != self.api_key;
        let language_changed = next.language != self.language;
        let proxy_changed = next.use_system_proxy != self.use_system_proxy;
        if key_changed || language_changed || proxy_changed {
            return TmdbChange::Catalog;
        }

        if next.poster_size != self.poster_size {
            return TmdbChange::PosterSize;
        }
        TmdbChange::None
    }
}

pub struct App {
    nav: Nav,
    settings: Settings,
    store: Option<SettingsStore>,
    load_error: Option<String>,
    save_error: Option<String>,
    probes: Probes,
    speed_mb: u32,
    home: HomeState,
    media: Option<MediaState>,
    person: Option<PersonState>,
    torrents: Option<TorrentState>,
    posters: PosterMap,
    images: ExtraImages,
    last_tmdb: TmdbView,
    scroll: ScrollFlash,
    torrent_intro: Animation<bool>,
    spin_tick: u64,
    player: Option<PlayerState>,
    engine: Option<cinebox_player::Engine>,
}

fn open_settings_store() -> (Option<SettingsStore>, Settings, Option<String>) {
    let store = match SettingsStore::system() {
        Ok(store) => store,
        Err(error) => {
            error!(%error, "settings store unavailable");
            return (None, Settings::default(), Some(error.to_string()));
        }
    };
    match store.load() {
        Ok(settings) => {
            info!(path = %store.path().display(), "settings loaded");
            if !store.path().exists()
                && let Err(error) = store.save(&settings)
            {
                warn!(%error, "could not write default settings");
            }
            (Some(store), settings, None)
        }
        Err(error) => {
            error!(%error, "failed to load settings");
            (Some(store), Settings::default(), Some(error.to_string()))
        }
    }
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let (store, settings, load_error) = open_settings_store();
        let last_tmdb = TmdbView::from_settings(&settings);
        let (home, task) = if last_tmdb.has_key() {
            (HomeState::Loading, load_home_task(&settings))
        } else {
            (HomeState::NeedKey, Task::none())
        };

        (
            Self {
                nav: Nav::new(),
                settings,
                store,
                load_error,
                save_error: None,
                probes: Probes::default(),
                speed_mb: cinebox_torrserver::SPEED_TEST_SIZES_MB[0],
                home,
                media: None,
                person: None,
                torrents: None,
                posters: PosterMap::new(),
                images: HashMap::new(),
                last_tmdb,
                scroll: ScrollFlash::default(),
                torrent_intro: Animation::new(true),
                spin_tick: 0,
                player: None,
                engine: None,
            },
            task,
        )
    }

    pub fn title(&self) -> String {
        Msg::AppTitle.en().to_owned()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let intro = matches!(self.nav.current(), Screen::Torrents { .. })
            && self.torrent_intro.is_animating(Instant::now());
        let on_player = matches!(self.nav.current(), Screen::Player { .. });
        let mut subs = Vec::new();
        if on_player {
            subs.push(iced::time::every(Duration::from_millis(250)).map(|_| Message::PlayerTick));
            subs.push(event::listen_with(player_window_events));
        }
        if self.scroll.needs_tick() || intro || self.files_spinning() {
            subs.push(iced::time::every(Duration::from_millis(16)).map(Message::ScrollFrame));
        }
        Subscription::batch(subs)
    }

    pub fn theme(&self) -> iced::Theme {
        iced::Theme::Dark
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenSettings => {
                self.scroll.reset();
                self.nav.push(Screen::Settings);
                Task::none()
            }
            Message::GoBack => {
                if matches!(self.nav.current(), Screen::Player { .. }) {
                    self.stop_player();
                    self.scroll.reset();
                    self.nav.pop();
                    return Task::none();
                }
                if self.leave_files_if_open() {
                    self.scroll.reset();
                    return Task::none();
                }
                self.scroll.reset();
                self.nav.pop();
                self.sync_screen()
            }
            Message::RetryHome => self.reload_home(),
            Message::RetryMedia => match self.nav.current() {
                Screen::Media { kind, id } => self.load_media(kind, id),
                _ => Task::none(),
            },
            Message::RetryPerson => match self.nav.current() {
                Screen::Person { id } => self.load_person(id),
                _ => Task::none(),
            },
            Message::RetryTorrents => match self.nav.current() {
                Screen::Torrents { kind, id } => self.load_torrents(kind, id),
                _ => Task::none(),
            },
            Message::OpenMedia { kind, id } => {
                if self.scroll.suppress_click {
                    return Task::none();
                }
                match kind {
                    MediaKind::Person => self.open_person(id),
                    MediaKind::Movie | MediaKind::Tv => self.open_media(kind, id),
                }
            }
            Message::OpenPerson { id } => {
                if self.scroll.suppress_click {
                    return Task::none();
                }
                self.open_person(id)
            }
            Message::WatchTorrents => self.open_torrents(),
            Message::Torrents(event) => self.on_torrent_event(event),
            Message::TorrentsLoaded { kind, id, result } => {
                self.on_torrents_loaded(kind, id, result)
            }
            Message::TorrentOpened {
                kind,
                id,
                seq,
                result,
            } => self.on_torrent_opened(kind, id, seq, result),
            Message::StreamReady {
                kind,
                id,
                seq,
                file_id,
                result,
            } => self.on_stream_ready(kind, id, seq, file_id, result),
            Message::Player(event) => self.on_player_event(event),
            Message::PlayerAttach(hwnd) => self.on_player_attach(hwnd),
            Message::PlayerTick => self.on_player_tick(),
            Message::PlayerResized => {
                if let Some(engine) = &self.engine {
                    engine.relayout();
                }
                Task::none()
            }
            Message::OpenUrl(url) => {
                if let Err(error) = open::that(&url) {
                    warn!(%error, "failed to open url");
                }
                Task::none()
            }
            Message::Settings(msg) => {
                let ui::settings::Update { persist, task } = ui::settings::update(
                    &mut self.settings,
                    &mut self.probes,
                    &mut self.speed_mb,
                    msg,
                );

                if persist {
                    self.persist();
                }

                task.map(Message::Settings)
            }
            Message::HomeLoaded(result) => self.on_home_loaded(result),
            Message::MediaLoaded { kind, id, result } => self.on_media_loaded(kind, id, result),
            Message::PersonLoaded { id, result } => self.on_person_loaded(id, result),
            Message::PosterLoaded { key, result } => {
                insert_poster(&mut self.posters, key, result);
                Task::none()
            }
            Message::ImageLoaded { url, result } => {
                insert_image(&mut self.images, url, result);
                Task::none()
            }
            Message::ScrollPan { pane, dx } => {
                self.scroll.stop(pane);
                self.scroll.touch(pane);
                scroll::scroll_by(pane, dx, 0.0)
            }
            Message::ScrollImpulse { pane, dx, dy, gain } => {
                self.scroll.impulse(pane, dx, dy, gain);
                scroll::scroll_by(pane, dx * 0.2, dy * 0.2)
            }
            Message::ScrollFlick { pane, vx, vy } => {
                self.scroll.flick(pane, vx, vy);
                Task::none()
            }
            Message::ScrollDragging(dragging) => {
                self.scroll.suppress_click = dragging;
                Task::none()
            }
            Message::ScrollFrame(now) => {
                if self.files_spinning() {
                    self.spin_tick = self.spin_tick.wrapping_add(1);
                }
                let motions = self.scroll.step(now);
                Task::batch(
                    motions
                        .into_iter()
                        .map(|(pane, dx, dy)| scroll::scroll_by(pane, dx, dy)),
                )
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body = match self.nav.current() {
            Screen::Home => ui::home::view(&self.home, &self.posters, &self.scroll),
            Screen::Settings => ui::settings::view(
                self.store.as_ref().map(SettingsStore::path),
                self.load_error.as_deref(),
                self.save_error.as_deref(),
                &self.settings,
                &self.probes,
                self.speed_mb,
                self.scroll.page(),
            ),
            Screen::Media { .. } => match &self.media {
                Some(state) => ui::card::view(
                    state,
                    &self.posters,
                    &self.images,
                    self.settings.tmdb.poster_size,
                    &self.scroll,
                ),
                None => ui::card::loading(),
            },
            Screen::Person { .. } => match &self.person {
                Some(state) => {
                    ui::person::view(state, &self.posters, &self.images, self.scroll.page())
                }
                None => ui::person::loading(),
            },
            Screen::Torrents { .. } => match &self.torrents {
                Some(state) => ui::torrents::view(
                    state,
                    &self.posters,
                    &self.images,
                    self.settings.tmdb.poster_size,
                    self.torrent_intro.interpolate(0.0, 1.0, Instant::now()),
                    &self.scroll,
                ),
                None => ui::torrents::loading(),
            },
            Screen::Player { .. } => match &self.player {
                Some(state) => player::view(state),
                None => ui::torrents::loading(),
            },
        };
        let wallpaper = match &self.media {
            Some(state)
                if matches!(
                    self.nav.current(),
                    Screen::Media { .. } | Screen::Torrents { .. }
                ) =>
            {
                ui::card::wallpaper(state, &self.images)
            }
            _ => None,
        };
        let overlay = self.files_overlay();
        ui::chrome::view(self.nav.current(), body, wallpaper, overlay)
    }

    fn files_overlay(&self) -> Option<Element<'_, Message>> {
        if !matches!(self.nav.current(), Screen::Torrents { .. }) {
            return None;
        }

        let state = self.torrents.as_ref()?;
        if !state.files.is_open() {
            return None;
        }

        Some(ui::torrents::files_overlay(
            state,
            &self.images,
            &self.posters,
            self.settings.tmdb.poster_size,
            self.scroll.files(),
            self.spin_tick,
        ))
    }

    fn showing_media(&self, kind: MediaKind, id: TmdbId) -> bool {
        self.media
            .as_ref()
            .is_some_and(|state| state.matches(kind, id))
    }

    fn showing_person(&self, id: TmdbId) -> bool {
        self.person.as_ref().is_some_and(|state| state.matches(id))
    }

    fn showing_torrents(&self, kind: MediaKind, id: TmdbId) -> bool {
        self.torrents
            .as_ref()
            .is_some_and(|state| state.matches(kind, id))
    }

    fn persist(&mut self) {
        self.save_error = None;
        let Some(store) = &self.store else {
            return;
        };

        if let Err(error) = store.save(&self.settings) {
            error!(%error, "failed to save settings");
            self.save_error = Some(error.to_string());
        }
    }

    fn sync_screen(&mut self) -> Task<Message> {
        match self.nav.current() {
            Screen::Home => self.sync_home(),
            Screen::Settings => Task::none(),
            Screen::Media { kind, id } => self.ensure_media(kind, id),
            Screen::Person { id } => self.ensure_person(id),
            Screen::Torrents { kind, id } => self.ensure_torrents(kind, id),
            Screen::Player { .. } => Task::none(),
        }
    }

    fn open_media(&mut self, kind: MediaKind, id: TmdbId) -> Task<Message> {
        self.scroll.reset();
        self.nav.push(Screen::Media { kind, id });
        self.ensure_media(kind, id)
    }

    fn open_person(&mut self, id: TmdbId) -> Task<Message> {
        self.scroll.reset();
        self.nav.push(Screen::Person { id });
        self.ensure_person(id)
    }

    fn ensure_media(&mut self, kind: MediaKind, id: TmdbId) -> Task<Message> {
        if self.showing_media(kind, id) {
            return Task::none();
        }
        self.load_media(kind, id)
    }

    fn ensure_person(&mut self, id: TmdbId) -> Task<Message> {
        if self.showing_person(id) {
            return Task::none();
        }
        self.load_person(id)
    }

    fn load_media(&mut self, kind: MediaKind, id: TmdbId) -> Task<Message> {
        self.media = Some(MediaState::Loading { kind, id });
        load_media_task(&self.settings, kind, id)
    }

    fn load_person(&mut self, id: TmdbId) -> Task<Message> {
        self.person = Some(PersonState::Loading { id });
        load_person_task(&self.settings, id)
    }

    fn open_torrents(&mut self) -> Task<Message> {
        let Some(MediaState::Ready(details)) = &self.media else {
            return Task::none();
        };
        let kind = details.kind;
        let id = details.id;
        self.scroll.reset();
        self.play_torrent_intro();
        self.nav.push(Screen::Torrents { kind, id });
        self.load_torrents(kind, id)
    }

    fn play_torrent_intro(&mut self) {
        self.torrent_intro = Animation::new(false)
            .duration(Duration::from_millis(480))
            .easing(Easing::EaseOutCubic);

        self.torrent_intro.go_mut(true, Instant::now());
    }

    fn ensure_torrents(&mut self, kind: MediaKind, id: TmdbId) -> Task<Message> {
        if self.showing_torrents(kind, id) {
            return Task::none();
        }
        self.load_torrents(kind, id)
    }

    fn load_torrents(&mut self, kind: MediaKind, id: TmdbId) -> Task<Message> {
        let (mut state, task) = {
            let Some(MediaState::Ready(details)) = &self.media else {
                return Task::none();
            };

            if details.kind != kind || details.id != id {
                return Task::none();
            }

            let state = TorrentState::from_details(details);
            let has_parser = !self.settings.parser.url.trim().is_empty();
            let task = has_parser.then(|| load_torrents_task(&self.settings, details));
            (state, task)
        };

        if task.is_none() {
            state.hits = TorrentHits::Failed(Msg::NeedParser.en().to_owned());
        }

        self.torrents = Some(state);
        task.unwrap_or_else(Task::none)
    }

    fn leave_files_if_open(&mut self) -> bool {
        let on_torrents = matches!(self.nav.current(), Screen::Torrents { .. });
        let Some(state) = &mut self.torrents else {
            return false;
        };

        if !on_torrents || !state.files.is_open() {
            return false;
        }

        state.files.close();
        true
    }

    fn files_spinning(&self) -> bool {
        if !matches!(self.nav.current(), Screen::Torrents { .. }) {
            return false;
        }

        self.torrents
            .as_ref()
            .is_some_and(|state| state.files.is_spinning())
    }

    fn on_torrent_event(&mut self, event: TorrentEvent) -> Task<Message> {
        match event {
            TorrentEvent::Pick(index) => self.pick_torrent(index),
            TorrentEvent::PickFile(file_id) => self.pick_file(file_id),
            TorrentEvent::CloseFiles => {
                if let Some(state) = &mut self.torrents {
                    state.files.close();
                }
                Task::none()
            }
            TorrentEvent::KeepOpen => Task::none(),
            TorrentEvent::RetryFiles => self.retry_files(),
            other => {
                if let Some(state) = &mut self.torrents {
                    ui::torrents::update(state, other, self.settings.player.default_quality);
                }
                Task::none()
            }
        }
    }

    fn pick_torrent(&mut self, index: usize) -> Task<Message> {
        let Some(state) = &mut self.torrents else {
            return Task::none();
        };

        let TorrentHits::Ready(hits) = &state.hits else {
            return Task::none();
        };

        let Some(hit) = hits.get(index) else {
            return Task::none();
        };

        if hit.magnet.is_empty() {
            return Task::none();
        }

        let url_missing = self.settings.torrserver.url.trim().is_empty();
        if url_missing {
            state.files = FilesPane::Failed(Msg::NeedTorrServer.en().to_owned());
            return Task::none();
        }

        let category = match state.kind {
            MediaKind::Tv => "tv",
            MediaKind::Movie | MediaKind::Person => "movie",
        };

        let poster = tmdb_image_url(
            state.movie.poster_path.as_deref(),
            self.settings.tmdb.poster_size.tmdb_path(),
        )
        .unwrap_or_default();

        let spec = cinebox_torrserver::AddSpec {
            link: hit.magnet.clone(),
            title: state.movie.title.clone(),
            poster,
            category: category.to_owned(),
            save_to_db: self.settings.torrserver.save_to_db,
        };
        state.pending_add = Some(spec.clone());
        state.pick_gen += 1;
        let seq = state.pick_gen;
        let kind = state.kind;
        let id = state.id;
        let movie = state.movie.clone();
        let runtime = state.runtime_minutes;
        state.files = FilesPane::Loading;
        open_magnet_task(&self.settings, spec, movie, kind, id, runtime, seq)
    }

    fn retry_files(&mut self) -> Task<Message> {
        let Some(state) = &mut self.torrents else {
            return Task::none();
        };

        let Some(spec) = state.pending_add.clone() else {
            return Task::none();
        };

        let url_missing = self.settings.torrserver.url.trim().is_empty();
        if url_missing {
            state.files = FilesPane::Failed(Msg::NeedTorrServer.en().to_owned());
            return Task::none();
        }

        state.pick_gen += 1;
        let seq = state.pick_gen;
        let kind = state.kind;
        let id = state.id;
        let movie = state.movie.clone();
        let runtime = state.runtime_minutes;
        state.files = FilesPane::Loading;
        open_magnet_task(&self.settings, spec, movie, kind, id, runtime, seq)
    }

    fn pick_file(&mut self, file_id: i32) -> Task<Message> {
        let Some(state) = &mut self.torrents else {
            return Task::none();
        };

        let wait = self.settings.torrserver.wait_preload;
        let (path, hash, files) = {
            let Some(ready) = state.files.ready_or_preload() else {
                return Task::none();
            };
            let Some(file) = ready.files.iter().find(|file| file.id == file_id) else {
                return Task::none();
            };
            let files = wait.then(|| ready.clone());
            (file.path.clone(), ready.hash.clone(), files)
        };

        let kind = state.kind;
        let id = state.id;
        let seq = state.pick_gen;
        if let Some(files) = files {
            state.files = FilesPane::Preloading { files, file_id };
        }

        wait_stream_task(&self.settings, path, hash, file_id, kind, id, seq)
    }

    fn on_torrent_opened(
        &mut self,
        kind: MediaKind,
        id: TmdbId,
        seq: u64,
        result: Result<ui::torrents::ReadyFiles, String>,
    ) -> Task<Message> {
        if !self.showing_torrents(kind, id) {
            return Task::none();
        }

        let Some(state) = &mut self.torrents else {
            return Task::none();
        };

        if state.pick_gen != seq {
            return Task::none();
        }

        let ready = match result {
            Ok(ready) => ready,
            Err(error) => {
                error!(%error, "failed to add torrent");
                state.files = FilesPane::Failed(error);
                return Task::none();
            }
        };

        info!(n = ready.files.len(), "torrent files ready");
        let stills: Vec<String> = ready
            .files
            .iter()
            .filter_map(|file| file.still_url.clone())
            .collect();
        let scroll_y = ready.resume_index().unwrap_or(0) as f32 * 110.0;
        state.files = FilesPane::Ready(ready);
        let images = queue_plain_urls(&self.images, &self.settings, stills);
        if scroll_y <= 0.0 {
            return images;
        }

        Task::batch([
            images,
            scroll::scroll_by(scroll::ScrollPane::Files, 0.0, scroll_y),
        ])
    }

    fn on_stream_ready(
        &mut self,
        kind: MediaKind,
        id: TmdbId,
        seq: u64,
        file_id: i32,
        result: Result<String, String>,
    ) -> Task<Message> {
        if !self.showing_torrents(kind, id) {
            return Task::none();
        }

        let Some(state) = &mut self.torrents else {
            return Task::none();
        };

        if state.pick_gen != seq {
            return Task::none();
        }

        let url = match result {
            Ok(url) => url,
            Err(error) => {
                error!(%error, "stream not ready");
                state.files = FilesPane::Failed(error);
                return Task::none();
            }
        };

        info!(file_id, "stream url ready");
        let mut files = match &state.files {
            FilesPane::Ready(files) => files.clone(),
            FilesPane::Preloading { files, .. } => files.clone(),
            FilesPane::Closed | FilesPane::Loading | FilesPane::Failed(_) => {
                return Task::none();
            }
        };

        files.selected_id = Some(file_id);
        let file_index = files
            .files
            .iter()
            .position(|file| file.id == file_id)
            .unwrap_or(0);

        let row = files.files.get(file_index);
        let title = row
            .map(|file| file.title.clone())
            .unwrap_or_else(|| state.movie.title.clone());

        let start = row.map(|file| file.timecode).unwrap_or(0.0);
        let hash = files.hash.clone();
        let rows = files.files.clone();

        state.files = FilesPane::Ready(files);
        state.files.close();

        self.stop_player();
        self.player = Some(PlayerState {
            title,
            hash,
            files: rows,
            file_index,
            paused: false,
            time: start,
            duration: 0.0,
            error: None,
            aid: 0,
            sid: 0,
            play_url: url,
        });

        self.nav.push(Screen::Player { kind, id });
        grab_parent_hwnd()
    }

    fn stop_player(&mut self) {
        self.engine = None;
        self.player = None;
    }

    fn on_player_attach(&mut self, hwnd: Option<isize>) -> Task<Message> {
        if !matches!(self.nav.current(), Screen::Player { .. }) {
            return Task::none();
        }

        let Some(hwnd) = hwnd else {
            if let Some(state) = &mut self.player {
                state.error = Some(String::from("Could not get the window handle."));
            }
            return Task::none();
        };

        match cinebox_player::Engine::attach(hwnd) {
            Ok(engine) => {
                let url = self.player.as_ref().map(|s| s.play_url.clone());
                let start = self.player.as_ref().map(|s| s.time).unwrap_or(0.0);
                self.engine = Some(engine);
                let Some(url) = url else {
                    return Task::none();
                };
                self.apply_load(&url, start)
            }
            Err(error) => {
                error!(%error, "mpv attach failed");
                if let Some(state) = &mut self.player {
                    state.error = Some(error.to_string());
                }
                Task::none()
            }
        }
    }

    fn apply_load(&mut self, url: &str, start: f64) -> Task<Message> {
        let header = cinebox_torrserver::mpv_http_header_fields(
            &self.settings.torrserver.username,
            self.settings.torrserver.password.expose(),
        );

        let opts = cinebox_player::PlayOpts {
            http_header_fields: header.as_deref(),
            loudnorm: self.settings.player.loudnorm,
            scale: self.settings.player.scale,
            start_seconds: start,
        };

        let Some(engine) = &self.engine else {
            return Task::none();
        };

        if let Err(error) = engine.load(url, opts) {
            error!(%error, "mpv load failed");
            if let Some(state) = &mut self.player {
                state.error = Some(error.to_string());
            }
        }

        Task::none()
    }

    fn on_player_event(&mut self, event: PlayerEvent) -> Task<Message> {
        match event {
            PlayerEvent::TogglePause => self.player_toggle(),
            PlayerEvent::SeekBack => self.player_seek(-cinebox_player::SEEK_SECS),
            PlayerEvent::SeekFwd => self.player_seek(cinebox_player::SEEK_SECS),
            PlayerEvent::CycleAudio => self.player_cmd(|engine| engine.cycle_audio()),
            PlayerEvent::CycleSubs => self.player_cmd(|engine| engine.cycle_subs()),
            PlayerEvent::Next => self.player_next(),
        }
    }

    fn player_toggle(&mut self) -> Task<Message> {
        let Some(engine) = &self.engine else {
            return Task::none();
        };

        match engine.toggle_pause() {
            Ok(paused) => {
                if let Some(state) = &mut self.player {
                    state.paused = paused;
                }
            }
            Err(error) => {
                error!(%error, "mpv pause failed");
            }
        }
        Task::none()
    }

    fn player_seek(&mut self, delta: f64) -> Task<Message> {
        let Some(engine) = &self.engine else {
            return Task::none();
        };

        if let Err(error) = engine.seek(delta) {
            error!(%error, "mpv seek failed");
        }

        Task::none()
    }

    fn player_cmd(
        &mut self,
        op: impl FnOnce(&cinebox_player::Engine) -> Result<(), cinebox_player::Error>,
    ) -> Task<Message> {
        let Some(engine) = &self.engine else {
            return Task::none();
        };

        if let Err(error) = op(engine) {
            error!(%error, "mpv command failed");
        }

        Task::none()
    }

    fn player_next(&mut self) -> Task<Message> {
        let next = {
            let Some(state) = &mut self.player else {
                return Task::none();
            };

            if !state.has_next() {
                return Task::none();
            }

            state.file_index += 1;
            let Some(file) = state.files.get(state.file_index).cloned() else {
                return Task::none();
            };

            state.title = file.title.clone();
            state.time = file.timecode;
            let hash = state.hash.clone();
            let url = match cinebox_torrserver::stream_url(
                &self.settings.torrserver.url,
                &file.path,
                &hash,
                file.id,
                cinebox_torrserver::StreamFlag::Play,
            ) {
                Ok(url) => url,
                Err(error) => {
                    state.error = Some(error.to_string());
                    return Task::none();
                }
            };

            state.play_url = url.clone();
            (url, file.timecode)
        };
        self.apply_load(&next.0, next.1)
    }

    fn on_player_tick(&mut self) -> Task<Message> {
        let Some(engine) = &self.engine else {
            return Task::none();
        };

        let snap = engine.snapshot();
        let auto = self.settings.player.auto_next;

        let go_next = {
            let Some(state) = &mut self.player else {
                return Task::none();
            };
            state.paused = snap.paused;
            state.time = snap.time;
            state.duration = snap.duration;
            state.aid = snap.aid;
            state.sid = snap.sid;
            let ended = snap.eof && snap.duration > 1.0;
            ended && auto && state.has_next()
        };

        if go_next {
            return self.player_next();
        }

        Task::none()
    }

    fn on_home_loaded(&mut self, result: Result<HomeCatalog, String>) -> Task<Message> {
        let catalog = match result {
            Ok(catalog) => catalog,
            Err(error) => {
                error!(%error, "failed to load home catalog");
                self.home = HomeState::Failed(error);
                return Task::none();
            }
        };

        info!("home catalog loaded");
        let task = queue_posters(&self.posters, &self.settings, &catalog);
        self.home = HomeState::Ready(catalog);
        task
    }

    fn on_media_loaded(
        &mut self,
        kind: MediaKind,
        id: TmdbId,
        result: Result<Box<MediaDetails>, String>,
    ) -> Task<Message> {
        if !self.showing_media(kind, id) {
            return Task::none();
        }

        let details = match result {
            Ok(details) => details,
            Err(error) => {
                error!(%error, "failed to load media details");
                self.media = Some(MediaState::Failed { kind, id, error });
                return Task::none();
            }
        };

        info!(id = id.get(), "media details loaded");
        let task = queue_media_assets(&self.posters, &self.images, &self.settings, &details);
        self.media = Some(MediaState::Ready(details));
        task
    }

    fn on_person_loaded(
        &mut self,
        id: TmdbId,
        result: Result<Box<PersonDetails>, String>,
    ) -> Task<Message> {
        if !self.showing_person(id) {
            return Task::none();
        }

        let details = match result {
            Ok(details) => details,
            Err(error) => {
                error!(%error, "failed to load person details");
                self.person = Some(PersonState::Failed { id, error });
                return Task::none();
            }
        };

        info!(id = id.get(), "person details loaded");
        let task = queue_person_assets(&self.posters, &self.images, &self.settings, &details);
        self.person = Some(PersonState::Ready(details));
        task
    }

    fn on_torrents_loaded(
        &mut self,
        kind: MediaKind,
        id: TmdbId,
        result: Result<Vec<cinebox_parse::TorrentHit>, String>,
    ) -> Task<Message> {
        if !self.showing_torrents(kind, id) {
            return Task::none();
        }

        let Some(state) = &mut self.torrents else {
            return Task::none();
        };

        match result {
            Ok(hits) => {
                info!(
                    n = hits.len(),
                    runtime_min = state.runtime_minutes,
                    size0 = hits.first().map(|hit| hit.size_bytes),
                    bitrate0 = hits.first().and_then(|hit| hit.bitrate_mbps),
                    "torrents loaded"
                );
                state.hits = TorrentHits::Ready(hits);
            }
            Err(error) => {
                error!(%error, "failed to search torrents");
                state.hits = TorrentHits::Failed(error);
            }
        }

        Task::none()
    }

    fn sync_home(&mut self) -> Task<Message> {
        let next = TmdbView::from_settings(&self.settings);
        if !next.has_key() {
            self.home = HomeState::NeedKey;
            self.posters.clear();
            self.last_tmdb = next;
            return Task::none();
        }

        let change = self.last_tmdb.change_from(&next);
        let need_catalog = matches!(self.home, HomeState::NeedKey);
        self.last_tmdb = next;

        match change {
            TmdbChange::Catalog => self.reload_home(),
            _ if need_catalog => self.reload_home(),
            TmdbChange::PosterSize => {
                self.posters.clear();
                self.images.clear();
                let HomeState::Ready(catalog) = &self.home else {
                    return Task::none();
                };
                queue_posters(&self.posters, &self.settings, catalog)
            }
            TmdbChange::None => Task::none(),
        }
    }

    fn reload_home(&mut self) -> Task<Message> {
        if self.settings.tmdb.api_key.is_empty() {
            self.home = HomeState::NeedKey;
            self.posters.clear();
            return Task::none();
        }

        self.home = HomeState::Loading;
        self.posters.clear();
        self.last_tmdb = TmdbView::from_settings(&self.settings);
        load_home_task(&self.settings)
    }
}

fn grab_parent_hwnd() -> Task<Message> {
    window::latest().then(|id| {
        let Some(id) = id else {
            return Task::done(Message::PlayerAttach(None));
        };
        window::run(id, parent_hwnd).map(Message::PlayerAttach)
    })
}

fn parent_hwnd(window: &dyn Window) -> Option<isize> {
    use window::raw_window_handle::RawWindowHandle;
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win) => Some(win.hwnd.get()),
        _ => None,
    }
}

fn player_window_events(event: IcedEvent, status: EventStatus, _id: window::Id) -> Option<Message> {
    if status == EventStatus::Captured {
        return None;
    }
    match event {
        IcedEvent::Window(window::Event::Resized(_)) => Some(Message::PlayerResized),
        IcedEvent::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key {
            Key::Named(Named::Space) => Some(Message::Player(PlayerEvent::TogglePause)),
            Key::Named(Named::ArrowLeft) => Some(Message::Player(PlayerEvent::SeekBack)),
            Key::Named(Named::ArrowRight) => Some(Message::Player(PlayerEvent::SeekFwd)),
            Key::Named(Named::Escape) => Some(Message::GoBack),
            _ => None,
        },
        _ => None,
    }
}
