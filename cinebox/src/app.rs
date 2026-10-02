//! Thin dispatcher: navigation + shared services. Screens own their state.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cinebox_core::{
    PosterSize, SEARCH_HISTORY_LIMIT, Settings, UiLanguage, allowed_image_sizes, tmdb_image_url,
};
use egui::{CentralPanel, Frame};
use tracing::error;

use crate::images::ImageSlot;
use crate::nav::{Nav, NavAction, RailEntry, Screen};
use crate::screens::{
    CategoryScreen, DiscoverScreen, HomeScreen, LibraryScreen, LiveTmdb, MediaScreen,
    OnboardingScreen, PersonScreen, PlayerScreen, SearchScreen, SectionScreen, SettingsScreen,
    TorrentsScreen,
};
use crate::services::{Services, db_block_on};
use crate::settings_input::tmdb_key_hint;
use crate::theme::Theme;
use crate::widgets::search::SearchBar;
use crate::widgets::{backdrop, chrome, focus, rail};

struct TmdbView {
    api_key: String,
    language: UiLanguage,
    poster_size: PosterSize,
    net: cinebox_net::NetConfig,
}

enum TmdbChange {
    None,
    /// The key was removed: nothing can be fetched, cached TMDB data goes too.
    KeyCleared,
    /// Key or network changed: refetch everything, images included.
    Catalog,
    Language,
    PosterSize,
}

impl TmdbView {
    fn from_settings(settings: &Settings) -> Self {
        Self {
            api_key: settings.tmdb.api_key.expose().to_owned(),
            language: settings.general.language,
            poster_size: settings.tmdb.poster_size,
            net: crate::jobs::net_config(settings),
        }
    }

    fn change_from(&self, next: &Self) -> TmdbChange {
        let key_cleared = next.api_key.is_empty() && !self.api_key.is_empty();
        if key_cleared {
            return TmdbChange::KeyCleared;
        }

        // Without a usable key there is nothing to reload; a malformed one would
        // only earn a 401 and a flash of the error page.
        let key_unusable = next.api_key.is_empty() || tmdb_key_hint(&next.api_key).is_some();
        if key_unusable {
            return TmdbChange::None;
        }

        let key_changed = next.api_key != self.api_key;
        let language_changed = next.language != self.language;
        let net_changed = next.net != self.net;

        if key_changed || net_changed {
            return TmdbChange::Catalog;
        }

        if language_changed {
            return TmdbChange::Language;
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
    section: SectionScreen,
    category: CategoryScreen,
    discover: DiscoverScreen,
    library: LibraryScreen,
    search: SearchScreen,
    search_bar: SearchBar,
    settings_screen: SettingsScreen,
    onboarding: OnboardingScreen,
    media: MediaScreen,
    person: PersonScreen,
    torrents: TorrentsScreen,
    player: PlayerScreen,
    /// The window had focus last frame (TV backgrounding).
    foreground: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, host: crate::platform::Host) -> Self {
        crate::platform::set(&cc.egui_ctx, host);
        let theme = Theme::dark();
        theme.apply(&cc.egui_ctx);
        crate::fonts::install(&cc.egui_ctx);
        egui_material_icons::initialize(&cc.egui_ctx);
        cc.egui_ctx
            .plugin_or_default::<egui_async::EguiAsyncPlugin>();

        let engine = attach_engine(cc);
        let mut services = Services::boot(engine);
        let first_run = !services.settings.general.onboarded;
        if first_run {
            adopt_system_language(&mut services);
        }

        let last_tmdb = TmdbView::from_settings(&services.settings);
        crate::i18n::apply(services.settings.general.language);

        let now = cc.egui_ctx.input(|i| i.time);
        services.announce_boot_problems(now);

        let mut onboarding = OnboardingScreen::default();
        if first_run {
            onboarding.open(&services, now);
        }
        let search_bar = SearchBar::with_history(load_search_history(&services));

        Self {
            nav: Nav::new(),
            theme,
            services,
            last_tmdb,
            home: HomeScreen::default(),
            section: SectionScreen::default(),
            category: CategoryScreen::default(),
            discover: DiscoverScreen::default(),
            library: LibraryScreen::default(),
            search: SearchScreen::default(),
            search_bar,
            settings_screen: SettingsScreen::default(),
            onboarding,
            media: MediaScreen::default(),
            person: PersonScreen::default(),
            torrents: TorrentsScreen::default(),
            player: PlayerScreen::default(),
            foreground: true,
        }
    }

    fn apply_nav(&mut self, action: NavAction, now: f64, ctx: &egui::Context) {
        let before = self.nav.current();
        self.nav.mark_focus(ctx.memory(|mem| mem.focused()));
        self.apply_nav_action(action, now, ctx);

        let returned = self.nav.current() != before;
        if let Some(id) = self.nav.focus_mark().filter(|_| returned) {
            ctx.memory_mut(|mem| mem.request_focus(id));
        }
    }

    fn apply_nav_action(&mut self, action: NavAction, now: f64, ctx: &egui::Context) {
        match action {
            NavAction::OpenSettings => self.settings_screen.toggle(now),
            NavAction::OpenSettingsAt(page) => self.settings_screen.open_at(page, now),
            NavAction::OpenOnboarding => {
                self.settings_screen.close(now);
                self.onboarding.open(&self.services, now);
            }
            NavAction::GoBack => self.go_back(now, ctx),
            NavAction::OpenRail(entry) => {
                let screen = match entry {
                    RailEntry::Home => Screen::Home,
                    RailEntry::Section(section) => Screen::Section { section },
                    RailEntry::Library => Screen::Library,
                };
                self.nav.switch_top(screen);
            }
            NavAction::OpenCategory { id, items } => {
                self.nav.push(Screen::Category { id });
                self.category.seed(id, items);
            }
            NavAction::OpenDiscover { section, filters } => {
                self.nav.push(Screen::Discover { section });
                self.discover.seed(section, filters);
            }
            NavAction::OpenLibrary { list } => {
                self.nav.push(Screen::Library);
                self.library.seed(list);
            }
            NavAction::OpenSearch { query } => {
                self.search_bar.remember(&query);
                if let Some(db) = &self.services.db {
                    if let Err(error) = db_block_on(db.record_search(&query)) {
                        error!(%error, "failed to record search");
                    }
                }

                self.search.seed(query);
                if !matches!(self.nav.current(), Screen::Search) {
                    self.nav.push(Screen::Search);
                }
            }
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
                let Screen::Media { kind, id } = self.nav.current() else {
                    return;
                };
                let Some(details) = self.media.ready() else {
                    return;
                };

                self.torrents
                    .ensure_open(details, &self.services.settings.parser.default_quality);
                self.nav.push(Screen::Torrents { kind, id });
            }
        }
    }

    fn go_back(&mut self, now: f64, ctx: &egui::Context) {
        if self.settings_screen.on_back(now) {
            return;
        }

        if matches!(self.nav.current(), Screen::Player { .. }) {
            self.player.stop(&mut self.services, ctx);
            self.nav.pop();
            return;
        }

        let on_media = matches!(self.nav.current(), Screen::Media { .. });
        if on_media && self.media.on_back() {
            return;
        }

        let on_discover = matches!(self.nav.current(), Screen::Discover { .. });
        if on_discover && self.discover.on_back(now) {
            return;
        }

        if self.torrents.on_back(now) {
            return;
        }

        // A remote's Back on the first screen leaves the app, like any TV app.
        if self.nav.is_root() && crate::platform::is_tv(ctx) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.nav.pop();
    }

    /// `true` when none of the TMDB-relevant settings changed since the last
    /// sync; checked without cloning the api key string.
    fn tmdb_unchanged(&self) -> bool {
        let settings = &self.services.settings;
        let last = &self.last_tmdb;

        last.api_key == settings.tmdb.api_key.expose()
            && last.language == settings.general.language
            && last.poster_size == settings.tmdb.poster_size
            && last.net == crate::jobs::net_config(settings)
    }

    fn sync_tmdb(&mut self) {
        if self.tmdb_unchanged() {
            return;
        }

        let next = TmdbView::from_settings(&self.services.settings);
        let change = self.last_tmdb.change_from(&next);
        self.last_tmdb = next;

        match change {
            TmdbChange::KeyCleared => {
                self.forget_live_tmdb();
                self.services.clear_tmdb_cache();
            }
            TmdbChange::Catalog => {
                self.forget_live_tmdb();
                self.services.images.clear();
            }
            TmdbChange::Language => self.forget_live_tmdb(),
            TmdbChange::PosterSize => self.gc_poster_images(),
            TmdbChange::None => {}
        }
    }

    fn live_tmdb_screens(&mut self) -> [&mut dyn LiveTmdb; 7] {
        [
            &mut self.home,
            &mut self.section,
            &mut self.category,
            &mut self.discover,
            &mut self.search,
            &mut self.media,
            &mut self.person,
        ]
    }

    fn forget_live_tmdb(&mut self) {
        for screen in self.live_tmdb_screens() {
            screen.forget_live();
        }
    }

    fn gc_poster_images(&mut self) {
        self.services.images.clear();
        let Some(db) = &self.services.db else {
            return;
        };

        let sizes = allowed_image_sizes(self.services.settings.tmdb.poster_size);
        if let Err(error) = db_block_on(db.gc_images(&sizes)) {
            error!(%error, "failed to gc tmdb images");
        }
    }

    fn take_pending_play(&mut self, ctx: &egui::Context) {
        let req = self.torrents.take_play().or_else(|| self.media.take_play());
        let Some(req) = req else {
            return;
        };

        let id = req.card.id;
        let kind = req.card.kind;

        self.player.start(req, &mut self.services, ctx);
        self.nav.push(Screen::Player { kind, id });
    }

    /// Home button or another app on top: the film should not keep playing unseen.
    fn pause_in_background(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.viewport().focused);
        let backgrounded = focused == Some(false) && self.foreground;
        self.foreground = focused != Some(false);

        let on_player = matches!(self.nav.current(), Screen::Player { .. });
        if backgrounded && on_player {
            self.player.pause(&self.services);
        }
    }

    fn paint_backdrop(&mut self, ui: &mut egui::Ui) {
        let url = match self.nav.current() {
            Screen::Media { .. } | Screen::Torrents { .. } => self
                .media
                .ready()
                .and_then(|d| tmdb_image_url(d.backdrop_path.as_deref(), "w1280")),
            _ => None,
        };
        if let Some(url) = url {
            if let ImageSlot::Ready(tex) = self.services.images.backdrop(Some(&url)) {
                backdrop::paint(ui, tex, &self.theme);
            }
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        chrome::release_after_os_grab(ctx, raw_input);
        focus::before_pass(ctx, raw_input);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if crate::platform::is_tv(ctx) {
            fit_tv_zoom(ctx);
            self.pause_in_background(ctx);
        }

        let language = self.services.settings.general.language;
        if self.last_tmdb.language != language {
            crate::i18n::apply(language);
        }

        let net = crate::jobs::net_config(&self.services.settings);
        self.services.images.poll(ctx, &net);

        self.sync_tmdb();
        if self.services.take_home_refresh() {
            self.home.refresh();
        }

        if matches!(self.nav.current(), Screen::Player { .. }) {
            self.player.tick(&mut self.services, ctx);
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().plugin_or_default::<egui_async::EguiAsyncPlugin>();

        let mut action = None;
        let screen = self.nav.current();
        let theme = self.theme.clone();
        let on_player = matches!(screen, Screen::Player { .. });
        let tv = crate::platform::is_tv(ui.ctx());

        // The wizard is dismissed with its own "Set up later" button only.
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if escape && !self.onboarding.is_open() {
            // Escape closes an open dropdown; on TV, Back while typing only leaves the field.
            let popup_consumed = egui::Popup::is_any_open(ui.ctx());
            let field_consumed = tv && focus::was_editing(ui.ctx());
            let search_consumed = self.search_bar.consume_escape(ui.ctx());
            let player_consumed = on_player && self.player.consume_escape(ui.ctx());
            let consumed = popup_consumed || field_consumed || search_consumed || player_consumed;

            if !consumed {
                action = Some(NavAction::GoBack);
            }
        }

        let player_fullscreen = on_player && self.player.is_fullscreen();
        let fill = if matches!(screen, Screen::Player { .. }) {
            theme.video_bg
        } else {
            theme.page_bg
        };

        // TVs may crop the picture edges; video stays full-bleed.
        let safe_area = if tv && !on_player {
            egui::Margin::symmetric(TV_SAFE_X, TV_SAFE_Y)
        } else {
            egui::Margin::ZERO
        };

        CentralPanel::default()
            .frame(Frame::new().fill(fill).inner_margin(safe_area))
            .show(ui, |ui| {
                self.paint_backdrop(ui);
                let with_rail = screen.shows_rail() && !player_fullscreen;
                // Inside the 1px window outline, starting at the title bar's bottom edge.
                let window = ui.max_rect();
                let rail_body = egui::Rect::from_min_max(
                    egui::pos2(window.left() + 1.0, window.top() + theme.title_bar_h),
                    egui::pos2(window.right(), window.bottom() - 1.0),
                );
                let back_x = with_rail.then(|| rail::column_center(rail_body.left()));

                if !player_fullscreen
                    && let Some(nav) = chrome::header(
                        ui,
                        screen,
                        &theme,
                        self.settings_screen.is_open(),
                        &mut self.search_bar,
                        back_x,
                    )
                {
                    action = Some(nav);
                }

                let pad = theme.pad.round() as i8;
                let left = if with_rail {
                    (rail::WIDTH + theme.pad).round() as i8
                } else {
                    pad
                };
                let content_margin = match screen {
                    Screen::Player { .. } => egui::Margin::ZERO,
                    Screen::Home => egui::Margin {
                        left,
                        right: pad,
                        top: 0,
                        bottom: pad,
                    },
                    _ => egui::Margin {
                        left,
                        right: pad,
                        top: 0,
                        bottom: 0,
                    },
                };

                if with_rail {
                    if let Some(entry) = rail::show(ui, rail_body, &theme, screen.rail_entry()) {
                        action = Some(NavAction::OpenRail(entry));
                    }
                }

                let content = Frame::new()
                    .inner_margin(content_margin)
                    .show(ui, |ui| screen_ui(self, ui, screen, &theme));
                focus::set_content(ui.ctx(), content.response.rect);
                let screen_action = content.inner;

                if !player_fullscreen && !tv {
                    chrome::resize_edges(ui, &theme);
                    chrome::window_outline(ui, &theme);
                }

                if let Some(nav) = self.settings_screen.ui(ui, &mut self.services, &theme) {
                    action = Some(nav);
                }

                if action.is_none() {
                    action = screen_action;
                }
            });

        let ctx = ui.ctx().clone();
        self.onboarding.ui(&ctx, &mut self.services, &theme);

        self.services.toasts.show(&ctx, &theme);
        focus::paint_ring(&ctx, &theme);
        self.take_pending_play(&ctx);
        if let Some(action) = action {
            self.apply_nav(action, ui.input(|i| i.time), &ctx);
        }

        self.services.images.end_frame();
    }
}

/// Logical width the TV layout is set for; 1080p panels report density 2.0,
/// which alone would leave only 960x540 points.
const TV_WIDTH_PT: f32 = 1280.0;

/// Android TV's recommended overscan margins at that width.
const TV_SAFE_X: i8 = 48;
const TV_SAFE_Y: i8 = 27;

fn fit_tv_zoom(ctx: &egui::Context) {
    let Some(native) = ctx.input(|i| i.viewport().native_pixels_per_point) else {
        return;
    };

    let width_px = ctx.content_rect().width() * ctx.pixels_per_point();
    if width_px <= 0.0 || native <= 0.0 {
        return;
    }

    let zoom = width_px / TV_WIDTH_PT / native;
    if (ctx.zoom_factor() - zoom).abs() > 0.01 {
        ctx.set_zoom_factor(zoom);
    }
}

fn screen_ui(app: &mut App, ui: &mut egui::Ui, screen: Screen, theme: &Theme) -> Option<NavAction> {
    if !matches!(screen, Screen::Torrents { .. }) {
        app.torrents.hide();
    }

    match screen {
        Screen::Home => app.home.ui(ui, &mut app.services, theme),
        Screen::Section { section } => app.section.ui(ui, &mut app.services, theme, section),
        Screen::Category { id } => app.category.ui(ui, &mut app.services, theme, id),
        Screen::Discover { section } => app.discover.ui(ui, &mut app.services, theme, section),
        Screen::Library => app.library.ui(ui, &app.services, theme),
        Screen::Search => app.search.ui(ui, &mut app.services, theme),
        Screen::Media { kind, id } => app.media.ui(ui, &mut app.services, theme, kind, id),
        Screen::Person { id } => app.person.ui(ui, &mut app.services, theme, id),
        Screen::Torrents { kind, id } => {
            let nav = app.torrents.ui(ui, &mut app.services, theme, kind, id);
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
    match cinebox_player::Engine::attach(loader, native_display(cc)) {
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

/// VAAPI shares decoded frames with GL only through the window's display.
/// The display belongs to winit's event loop, which outlives the app and its engine.
#[cfg(target_os = "linux")]
fn native_display(cc: &eframe::CreationContext<'_>) -> cinebox_player::NativeDisplay {
    use cinebox_player::NativeDisplay;
    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

    let Ok(handle) = cc.display_handle() else {
        return NativeDisplay::None;
    };

    match handle.as_raw() {
        RawDisplayHandle::Wayland(wayland) => NativeDisplay::Wayland(wayland.display),
        RawDisplayHandle::Xlib(xlib) => {
            xlib.display.map_or(NativeDisplay::None, NativeDisplay::X11)
        }
        _ => NativeDisplay::None,
    }
}

/// Windows decoders (NVDEC, D3D11VA copy-back) need no display.
#[cfg(not(target_os = "linux"))]
fn native_display(_cc: &eframe::CreationContext<'_>) -> cinebox_player::NativeDisplay {
    cinebox_player::NativeDisplay::None
}

/// A fresh install starts in the OS language when there is a translation for it.
fn adopt_system_language(services: &mut Services) {
    let Some(language) = crate::i18n::system_language() else {
        return;
    };

    services.settings.general.language = language;
}

fn load_search_history(svc: &Services) -> Vec<String> {
    let Some(db) = &svc.db else {
        return Vec::new();
    };

    db_block_on(db.recent_searches(SEARCH_HISTORY_LIMIT)).unwrap_or_default()
}
