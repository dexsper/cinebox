use std::collections::HashMap;
use std::time::{Duration, Instant};

use cinebox_core::i18n::Msg;
use cinebox_core::{
    HomeCatalog, MediaDetails, MediaKind, PersonDetails, PosterSize, Settings, SettingsStore,
    TmdbId,
};
use iced::animation::Easing;
use iced::{Animation, Element, Subscription, Task};
use tracing::{error, info, warn};

use crate::images::{
    insert_image, insert_poster, queue_media_assets, queue_person_assets, queue_posters,
};
use crate::loaders::{load_home_task, load_media_task, load_person_task, load_torrents_task};
use crate::nav::{Nav, Screen};
use crate::ui;
use crate::ui::card::MediaState;
use crate::ui::home::{ExtraImages, HomeState, PosterMap};
use crate::ui::person::PersonState;
use crate::ui::scroll::{self, ScrollFlash};
use crate::ui::settings::Probes;
use crate::ui::torrents::{TorrentHits, TorrentState};

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
        if self.scroll.needs_tick() || intro {
            return iced::time::every(Duration::from_millis(16)).map(Message::ScrollFrame);
        }
        Subscription::none()
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
            Message::Torrents(event) => {
                if let Some(state) = &mut self.torrents {
                    ui::torrents::update(state, event, self.settings.player.default_quality);
                }
                Task::none()
            }
            Message::TorrentsLoaded { kind, id, result } => {
                self.on_torrents_loaded(kind, id, result)
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
                    self.scroll.page(),
                ),
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
        ui::chrome::view(self.nav.current(), body, wallpaper)
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
