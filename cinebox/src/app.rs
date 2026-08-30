//! Thin dispatcher: navigation + shared services. Screens own their state.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cinebox_core::{PosterSize, Settings, allowed_image_sizes, tmdb_image_url};
use egui::{CentralPanel, Frame};
use tracing::error;

use crate::nav::{Nav, NavAction, Screen};
use crate::screens::{
    HomeScreen, MediaScreen, PersonScreen, PlayerScreen, SettingsScreen, TorrentsScreen,
};
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{backdrop, chrome};

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
    theme: Theme,
    services: Services,
    last_tmdb: TmdbView,
    home: HomeScreen,
    settings_screen: SettingsScreen,
    media: MediaScreen,
    person: PersonScreen,
    torrents: TorrentsScreen,
    player: PlayerScreen,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let theme = Theme::dark();
        theme.apply(&cc.egui_ctx);
        egui_material_icons::initialize(&cc.egui_ctx);
        cc.egui_ctx
            .plugin_or_default::<egui_async::EguiAsyncPlugin>();

        let engine = attach_engine(cc);
        let services = Services::boot(engine);
        let last_tmdb = TmdbView::from_settings(&services.settings);

        Self {
            nav: Nav::new(),
            theme,
            services,
            last_tmdb,
            home: HomeScreen::default(),
            settings_screen: SettingsScreen::default(),
            media: MediaScreen::default(),
            person: PersonScreen::default(),
            torrents: TorrentsScreen::default(),
            player: PlayerScreen::default(),
        }
    }

    fn apply_nav(&mut self, action: NavAction) {
        match action {
            NavAction::OpenSettings => self.nav.push(Screen::Settings),
            NavAction::GoBack => self.go_back(),
            NavAction::OpenMedia { item } => {
                self.nav.push(Screen::Media {
                    kind: item.kind,
                    id: item.id,
                });
                self.media.seed(item);
            }
            NavAction::OpenPerson { person } => {
                self.nav.push(Screen::Person { id: person.id });
                self.person.seed(person);
            }
            NavAction::WatchTorrents => {
                if let Screen::Media { kind, id } = self.nav.current() {
                    self.nav.push(Screen::Torrents { kind, id });
                }
            }
            NavAction::OpenUrl(url) => {
                if let Err(error) = open::that(&url) {
                    error!(%error, "failed to open url");
                }
            }
        }
    }

    fn go_back(&mut self) {
        if matches!(self.nav.current(), Screen::Player { .. }) {
            self.player.stop(&self.services);
            self.nav.pop();
            return;
        }
        if self.torrents.leave_files_if_open() {
            return;
        }
        self.nav.pop();
    }

    fn sync_tmdb(&mut self) {
        let next = TmdbView::from_settings(&self.services.settings);
        if next.api_key.is_empty() {
            let had_key = !self.last_tmdb.api_key.is_empty();
            self.last_tmdb = next;
            if had_key {
                self.home.refresh();
                if let Some(db) = &self.services.db
                    && let Err(error) = db.clear_tmdb()
                {
                    error!(%error, "failed to purge tmdb cache");
                }
                self.services.images.clear();
            }
            return;
        }

        let change = self.last_tmdb.change_from(&next);
        self.last_tmdb = next;
        match change {
            TmdbChange::Catalog => {
                self.home.refresh();
                self.services.images.clear();
            }
            TmdbChange::PosterSize => {
                self.services.images.clear();
                if let Some(db) = &self.services.db {
                    let sizes = allowed_image_sizes(self.services.settings.tmdb.poster_size);
                    if let Err(error) = db.gc_images(&sizes) {
                        error!(%error, "failed to gc tmdb images");
                    }
                }
            }
            TmdbChange::None => {}
        }
    }

    fn take_pending_play(&mut self) {
        let Some(req) = self.torrents.take_play() else {
            return;
        };

        let id = req.id;
        let kind = req.kind;

        self.player.start(req, &mut self.services);
        self.nav.push(Screen::Player { kind, id });
    }

    fn paint_backdrop(&mut self, ui: &mut egui::Ui) {
        let url = match self.nav.current() {
            Screen::Media { .. } | Screen::Torrents { .. } => self
                .media
                .ready()
                .and_then(|d| tmdb_image_url(d.backdrop_path.as_deref(), "w1280")),
            _ => None,
        };
        if let Some(url) = url
            && let Some(tex) = self.services.images.get(&url)
        {
            backdrop::paint(ui, tex, &self.theme);
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.services.images.poll(ctx);
        self.sync_tmdb();
        if self.services.take_home_refresh() {
            self.home.refresh();
        }

        if matches!(self.nav.current(), Screen::Player { .. }) {
            self.player.tick(&mut self.services);
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().plugin_or_default::<egui_async::EguiAsyncPlugin>();

        let mut action = None;
        let screen = self.nav.current();
        let theme = self.theme.clone();

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = Some(NavAction::GoBack);
        }

        let fill = if matches!(screen, Screen::Player { .. }) {
            theme.video_bg
        } else {
            theme.page_bg
        };

        CentralPanel::default()
            .frame(Frame::new().fill(fill))
            .show(ui, |ui| {
                self.paint_backdrop(ui);
                if let Some(nav) = chrome::header(ui, screen, &theme) {
                    action = Some(nav);
                }

                let pad = theme.pad.round() as i8;
                let content_margin = match screen {
                    Screen::Player { .. } => egui::Margin::ZERO,
                    Screen::Media { .. } | Screen::Person { .. } | Screen::Torrents { .. } => {
                        egui::Margin {
                            left: pad,
                            right: pad,
                            top: 0,
                            bottom: 0,
                        }
                    }
                    _ => egui::Margin {
                        left: pad,
                        right: pad,
                        top: 0,
                        bottom: pad,
                    },
                };

                let screen_action = Frame::new()
                    .inner_margin(content_margin)
                    .show(ui, |ui| screen_ui(self, ui, screen, &theme))
                    .inner;

                chrome::resize_edges(ui, &theme);
                chrome::window_outline(ui, &theme);

                if action.is_none() {
                    action = screen_action;
                }
            });

        self.services.toasts.show(ui.ctx(), &theme);
        self.take_pending_play();
        if let Some(action) = action {
            self.apply_nav(action);
        }
    }
}

fn screen_ui(app: &mut App, ui: &mut egui::Ui, screen: Screen, theme: &Theme) -> Option<NavAction> {
    if !matches!(screen, Screen::Torrents { .. }) {
        app.torrents.hide();
    }
    match screen {
        Screen::Home => app.home.ui(ui, &mut app.services, theme),
        Screen::Settings => app.settings_screen.ui(ui, &mut app.services, theme),
        Screen::Media { kind, id } => app.media.ui(ui, &mut app.services, theme, kind, id),
        Screen::Person { id } => app.person.ui(ui, &mut app.services, theme, id),
        Screen::Torrents { kind, id } => {
            let details = app.media.ready().cloned();
            let nav = app
                .torrents
                .ui(ui, &mut app.services, theme, kind, id, details.as_ref());
            if app.torrents.intro_animating(ui.input(|i| i.time)) {
                ui.ctx().request_repaint();
            }
            nav
        }
        Screen::Player { .. } => app.player.ui(ui, &mut app.services, theme),
    }
}

fn attach_engine(cc: &eframe::CreationContext<'_>) -> Option<Arc<Mutex<cinebox_player::Engine>>> {
    let loader = cc.get_proc_address.clone()?;
    match cinebox_player::Engine::attach(loader) {
        Ok(mut engine) => {
            let ctx = cc.egui_ctx.clone();
            engine.set_update_callback(move || ctx.request_repaint());
            Some(Arc::new(Mutex::new(engine)))
        }
        Err(error) => {
            error!(%error, "mpv render attach failed");
            None
        }
    }
}
