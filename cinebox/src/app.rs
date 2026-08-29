use std::collections::{HashMap, HashSet};

use cinebox_core::i18n::Msg;
use cinebox_core::{
    CatalogItem, HomeCatalog, MediaDetails, MediaKind, PersonDetails, PosterSize, Settings,
    SettingsStore, TmdbId, tmdb_image_url,
};
use iced::widget::image::Handle as ImageHandle;
use iced::{Element, Subscription, Task};
use tracing::{error, info, warn};

use crate::nav::{Nav, Screen};
use crate::ui;
use crate::ui::card::MediaState;
use crate::ui::home::{ExtraImages, HomeState, PosterMap};
use crate::ui::person::PersonState;
use crate::ui::scroll::{self, ScrollFlash, ScrollPane};
use crate::ui::settings::Probes;

#[derive(Debug, Clone)]
pub enum Message {
    OpenSettings,
    GoBack,
    RetryHome,
    RetryMedia,
    RetryPerson,
    OpenMedia {
        kind: MediaKind,
        id: TmdbId,
    },
    OpenPerson {
        id: TmdbId,
    },
    WatchTorrents,
    OpenUrl(String),
    Settings(ui::settings::Message),
    HomeLoaded(Result<HomeCatalog, String>),
    MediaLoaded {
        kind: MediaKind,
        id: TmdbId,
        result: Result<Box<MediaDetails>, String>,
    },
    PersonLoaded {
        id: TmdbId,
        result: Result<Box<PersonDetails>, String>,
    },
    PosterLoaded {
        key: (MediaKind, TmdbId),
        result: Result<Vec<u8>, String>,
    },
    ImageLoaded {
        url: String,
        result: Result<Vec<u8>, String>,
    },
    ScrollPan {
        pane: ScrollPane,
        dx: f32,
    },
    ScrollImpulse {
        pane: ScrollPane,
        dx: f32,
        dy: f32,
        gain: f32,
    },
    ScrollFlick {
        pane: ScrollPane,
        vx: f32,
        vy: f32,
    },
    ScrollDragging(bool),
    ScrollFrame(std::time::Instant),
}

struct TmdbView {
    api_key: String,
    language: Option<String>,
    poster_size: PosterSize,
    use_system_proxy: bool,
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
    posters: PosterMap,
    images: ExtraImages,
    torrent_hint: bool,
    last_tmdb: TmdbView,
    scroll: ScrollFlash,
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let (store, settings, load_error) = match SettingsStore::system() {
            Ok(store) => match store.load() {
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
            },
            Err(error) => {
                error!(%error, "settings store unavailable");
                (None, Settings::default(), Some(error.to_string()))
            }
        };

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
                posters: PosterMap::new(),
                images: HashMap::new(),
                torrent_hint: false,
                last_tmdb,
                scroll: ScrollFlash::default(),
            },
            task,
        )
    }

    pub fn title(&self) -> String {
        Msg::AppTitle.en().to_owned()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if self.scroll.needs_tick() {
            iced::time::every(std::time::Duration::from_millis(16)).map(Message::ScrollFrame)
        } else {
            Subscription::none()
        }
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
            Message::OpenMedia { kind, id } => {
                if self.scroll.suppress_click {
                    Task::none()
                } else if kind == MediaKind::Person {
                    self.open_person(id)
                } else {
                    self.open_media(kind, id)
                }
            }
            Message::OpenPerson { id } => {
                if self.scroll.suppress_click {
                    Task::none()
                } else {
                    self.open_person(id)
                }
            }
            Message::WatchTorrents => {
                self.torrent_hint = true;
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
            Message::HomeLoaded(result) => match result {
                Ok(catalog) => {
                    info!("home catalog loaded");
                    let task = self.queue_posters(&catalog);
                    self.home = HomeState::Ready(catalog);
                    task
                }
                Err(error) => {
                    error!(%error, "failed to load home catalog");
                    self.home = HomeState::Failed(error);
                    Task::none()
                }
            },
            Message::MediaLoaded { kind, id, result } => {
                if !self
                    .media
                    .as_ref()
                    .is_some_and(|state| state.matches(kind, id))
                {
                    return Task::none();
                }
                match result {
                    Ok(details) => {
                        info!(id = id.get(), "media details loaded");
                        let task = self.queue_media_assets(&details);
                        self.media = Some(MediaState::Ready(details));
                        task
                    }
                    Err(error) => {
                        error!(%error, "failed to load media details");
                        self.media = Some(MediaState::Failed { kind, id, error });
                        Task::none()
                    }
                }
            }
            Message::PersonLoaded { id, result } => {
                if !self.person.as_ref().is_some_and(|state| state.matches(id)) {
                    return Task::none();
                }
                match result {
                    Ok(details) => {
                        info!(id = id.get(), "person details loaded");
                        let task = self.queue_person_assets(&details);
                        self.person = Some(PersonState::Ready(details));
                        task
                    }
                    Err(error) => {
                        error!(%error, "failed to load person details");
                        self.person = Some(PersonState::Failed { id, error });
                        Task::none()
                    }
                }
            }
            Message::PosterLoaded { key, result } => {
                if let Ok(bytes) = result {
                    self.posters.insert(key, ImageHandle::from_bytes(bytes));
                }
                Task::none()
            }
            Message::ImageLoaded { url, result } => {
                if let Ok(bytes) = result {
                    self.images.insert(url, ImageHandle::from_bytes(bytes));
                }
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
                    self.torrent_hint,
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
        };
        ui::chrome::view(
            self.nav.current(),
            body,
            match &self.media {
                Some(state) if matches!(self.nav.current(), Screen::Media { .. }) => {
                    ui::card::wallpaper(state, &self.images)
                }
                _ => None,
            },
        )
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
        }
    }

    fn open_media(&mut self, kind: MediaKind, id: TmdbId) -> Task<Message> {
        self.scroll.reset();
        self.nav.push(Screen::Media { kind, id });
        self.torrent_hint = false;
        self.ensure_media(kind, id)
    }

    fn open_person(&mut self, id: TmdbId) -> Task<Message> {
        self.scroll.reset();
        self.nav.push(Screen::Person { id });
        self.ensure_person(id)
    }

    fn ensure_media(&mut self, kind: MediaKind, id: TmdbId) -> Task<Message> {
        if self
            .media
            .as_ref()
            .is_some_and(|state| state.matches(kind, id))
        {
            Task::none()
        } else {
            self.load_media(kind, id)
        }
    }

    fn ensure_person(&mut self, id: TmdbId) -> Task<Message> {
        if self.person.as_ref().is_some_and(|state| state.matches(id)) {
            Task::none()
        } else {
            self.load_person(id)
        }
    }

    fn load_media(&mut self, kind: MediaKind, id: TmdbId) -> Task<Message> {
        self.media = Some(MediaState::Loading { kind, id });
        self.torrent_hint = false;
        load_media_task(&self.settings, kind, id)
    }

    fn load_person(&mut self, id: TmdbId) -> Task<Message> {
        self.person = Some(PersonState::Loading { id });
        load_person_task(&self.settings, id)
    }

    fn sync_home(&mut self) -> Task<Message> {
        let next = TmdbView::from_settings(&self.settings);
        if !next.has_key() {
            self.home = HomeState::NeedKey;
            self.posters.clear();
            self.last_tmdb = next;
            return Task::none();
        }
        let lists_changed = next.api_key != self.last_tmdb.api_key
            || next.language != self.last_tmdb.language
            || next.use_system_proxy != self.last_tmdb.use_system_proxy;
        let size_changed = next.poster_size != self.last_tmdb.poster_size;
        self.last_tmdb = next;
        if lists_changed || matches!(self.home, HomeState::NeedKey) {
            self.reload_home()
        } else if size_changed {
            self.posters.clear();
            self.images.clear();
            match &self.home {
                HomeState::Ready(catalog) => {
                    let catalog = catalog.clone();
                    self.queue_posters(&catalog)
                }
                _ => Task::none(),
            }
        } else {
            Task::none()
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

    fn queue_posters(&self, catalog: &HomeCatalog) -> Task<Message> {
        self.queue_items(catalog.rows.iter().flat_map(|row| row.items.iter()))
    }

    fn queue_media_assets(&self, details: &MediaDetails) -> Task<Message> {
        let item = CatalogItem {
            id: details.id,
            kind: details.kind,
            title: details.title.clone(),
            year: details.year,
            vote: details.vote,
            poster_path: details.poster_path.clone(),
        };
        let posters = self.queue_items(
            std::iter::once(&item)
                .chain(details.collection.iter())
                .chain(details.recommendations.iter())
                .chain(details.similar.iter()),
        );
        let size = self.settings.tmdb.poster_size.tmdb_path();
        let mut extras = Vec::new();
        if let Some(url) = tmdb_image_url(details.poster_path.as_deref(), size) {
            extras.push((url, false));
        }
        if let Some(url) = tmdb_image_url(details.backdrop_path.as_deref(), "w1280") {
            extras.push((url, true));
        }
        for person in details.directors.iter().chain(details.cast.iter()) {
            if let Some(url) = tmdb_image_url(person.profile_path.as_deref(), "w185") {
                extras.push((url, false));
            }
        }
        Task::batch([posters, self.queue_urls(extras)])
    }

    fn queue_person_assets(&self, details: &PersonDetails) -> Task<Message> {
        let posters = self.queue_items(details.credits.iter());
        let mut urls = Vec::new();
        if let Some(url) = tmdb_image_url(details.profile_path.as_deref(), "w185") {
            urls.push((url, false));
        }
        Task::batch([posters, self.queue_urls(urls)])
    }

    fn queue_items<'a>(&self, items: impl IntoIterator<Item = &'a CatalogItem>) -> Task<Message> {
        let size = self.settings.tmdb.poster_size;
        let use_system_proxy = self.settings.interface.use_system_proxy;
        let tasks: Vec<_> = items
            .into_iter()
            .filter_map(|item| {
                if self.posters.contains_key(&(item.kind, item.id)) {
                    return None;
                }
                let url = item.poster_url(size)?;
                let key = (item.kind, item.id);
                Some(Task::perform(
                    async move {
                        cinebox_tmdb::download_image(&url, use_system_proxy)
                            .await
                            .map_err(|error| error.to_string())
                    },
                    move |result| Message::PosterLoaded { key, result },
                ))
            })
            .collect();
        Task::batch(tasks)
    }

    fn queue_urls(&self, urls: impl IntoIterator<Item = (String, bool)>) -> Task<Message> {
        let use_system_proxy = self.settings.interface.use_system_proxy;
        let mut seen = HashSet::new();
        let mut tasks = Vec::new();
        for (url, soften) in urls {
            if url.is_empty() || self.images.contains_key(&url) || !seen.insert(url.clone()) {
                continue;
            }
            let key = url.clone();
            tasks.push(Task::perform(
                async move {
                    let bytes = cinebox_tmdb::download_image(&url, use_system_proxy)
                        .await
                        .map_err(|error| error.to_string())?;
                    if !soften {
                        return Ok(bytes);
                    }
                    match crate::ui::backdrop::soften(&bytes) {
                        Ok(soft) => Ok(soft),
                        Err(error) => {
                            warn!(%error, "backdrop soften failed");
                            Ok(bytes)
                        }
                    }
                },
                move |result| Message::ImageLoaded { url: key, result },
            ));
        }
        Task::batch(tasks)
    }
}

fn load_home_task(settings: &Settings) -> Task<Message> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.tmdb.data_language.clone();
    let use_system_proxy = settings.interface.use_system_proxy;
    Task::perform(
        async move {
            cinebox_tmdb::fetch_home(&key, language.as_deref(), use_system_proxy)
                .await
                .map_err(|error| error.to_string())
        },
        Message::HomeLoaded,
    )
}

fn load_media_task(settings: &Settings, kind: MediaKind, id: TmdbId) -> Task<Message> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.tmdb.data_language.clone();
    let use_system_proxy = settings.interface.use_system_proxy;
    Task::perform(
        async move {
            cinebox_tmdb::fetch_media(&key, kind, id, language.as_deref(), use_system_proxy)
                .await
                .map(Box::new)
                .map_err(|error| error.to_string())
        },
        move |result| Message::MediaLoaded { kind, id, result },
    )
}

fn load_person_task(settings: &Settings, id: TmdbId) -> Task<Message> {
    let key = settings.tmdb.api_key.expose().to_owned();
    let language = settings.tmdb.data_language.clone();
    let use_system_proxy = settings.interface.use_system_proxy;
    Task::perform(
        async move {
            cinebox_tmdb::fetch_person(&key, id, language.as_deref(), use_system_proxy)
                .await
                .map(Box::new)
                .map_err(|error| error.to_string())
        },
        move |result| Message::PersonLoaded { id, result },
    )
}
