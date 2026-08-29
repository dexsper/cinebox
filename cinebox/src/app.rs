use cinebox_core::i18n::Msg;
use cinebox_core::{HomeCatalog, PosterSize, Settings, SettingsStore};
use iced::{Element, Task, Theme};
use tracing::{error, info, warn};

use crate::nav::{Nav, Screen};
use crate::ui;
use crate::ui::home::{HomeState, PosterMap};
use crate::ui::settings::Probes;

#[derive(Debug, Clone)]
pub enum Message {
    OpenSettings,
    GoBack,
    RetryHome,
    Settings(ui::settings::Message),
    HomeLoaded(Result<HomeCatalog, String>),
    PosterLoaded {
        key: (cinebox_core::MediaKind, cinebox_core::TmdbId),
        result: Result<Vec<u8>, String>,
    },
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
    posters: PosterMap,
    last_tmdb: TmdbView,
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
                posters: PosterMap::new(),
                last_tmdb,
            },
            task,
        )
    }

    pub fn title(&self) -> String {
        Msg::AppTitle.en().to_owned()
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenSettings => {
                self.nav.push(Screen::Settings);
                Task::none()
            }
            Message::GoBack => {
                self.nav.pop();
                if self.nav.current() == Screen::Home {
                    self.sync_home()
                } else {
                    Task::none()
                }
            }
            Message::RetryHome => self.reload_home(),
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
            Message::PosterLoaded { key, result } => {
                if let Ok(bytes) = result {
                    self.posters
                        .insert(key, iced::widget::image::Handle::from_bytes(bytes));
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body = match self.nav.current() {
            Screen::Home => ui::home::view(&self.home, &self.posters),
            Screen::Settings => ui::settings::view(
                self.store.as_ref().map(SettingsStore::path),
                self.load_error.as_deref(),
                self.save_error.as_deref(),
                &self.settings,
                &self.probes,
                self.speed_mb,
            ),
        };
        ui::chrome::view(self.nav.current(), body)
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
        let size = self.settings.tmdb.poster_size;
        let use_system_proxy = self.settings.interface.use_system_proxy;
        let tasks = catalog.rows.iter().flat_map(|row| {
            row.items.iter().filter_map(|item| {
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
        });
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
