//! Thin dispatcher: navigation + shared services. Screens own their state.

mod boot;
mod device;
pub mod nav;
pub(crate) mod routing;
mod shell;
mod tmdb;

use std::time::Duration;

use crate::platform::{self, DeviceEvent, Host};
use crate::screens::{
    CategoryScreen, DiscoverScreen, HomeScreen, LibraryScreen, MediaScreen, OnboardingScreen,
    PersonScreen, PlayerScreen, SearchScreen, SectionScreen, SettingsScreen, TorrentsScreen,
};
use crate::services::Services;
use crate::services::jobs;
use crate::theme::Theme;
use crate::widgets::search::SearchBar;
use crate::widgets::{chrome, focus};

use nav::{Nav, Screen};
use tmdb::TmdbView;

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
    /// The window had focus last frame (see `pause_in_background`).
    foreground: bool,
    /// Taken from the device before the frame, acted on in `logic`.
    device_events: Vec<DeviceEvent>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, host: Host) -> Self {
        let theme = Theme::dark().for_profile(host.profile);
        let platform_player = host.player.clone();

        platform::install(&cc.egui_ctx, host);
        theme.apply(&cc.egui_ctx);

        crate::theme::fonts::install(&cc.egui_ctx);

        cc.egui_ctx
            .plugin_or_default::<egui_async::EguiAsyncPlugin>();

        let player = platform_player.or_else(|| boot::attach_mpv(cc));
        if let Some(player) = &player {
            let ctx = cc.egui_ctx.clone();
            player.on_change(Box::new(move || ctx.request_repaint()));
        }

        let mut services = Services::boot(player);
        let first_run = !services.settings.general.onboarded;
        if first_run {
            boot::adopt_system_language(&mut services);
        }

        let last_tmdb = TmdbView::from_settings(&services.settings);
        crate::i18n::apply(services.settings.general.language);

        let now = cc.egui_ctx.input(|i| i.time);
        services.announce_boot_problems(now);

        let mut onboarding = OnboardingScreen::default();
        if first_run {
            onboarding.open(&services, now);
        }
        let search_bar = SearchBar::with_history(boot::load_search_history(&services));

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
            device_events: Vec::new(),
        }
    }
}

impl eframe::App for App {
    /// Clear wherever nothing is painted, so a video layer below the window shows there.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        chrome::release_after_os_grab(ctx, raw_input);
        let events = platform::take_input(ctx, raw_input);
        self.device_events.extend(events);
        focus::begin_frame(ctx, raw_input);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        platform::begin_frame(ctx);
        platform::set_overscan(ctx, self.services.settings.general.overscan);
        if !platform::profile(ctx).is_desktop_window() {
            self.pause_in_background(ctx);
        }

        self.handle_device_events(ctx);
        let net = jobs::net_config(&self.services.settings);
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

        let theme = self.theme.clone();
        let escape = self.back_on_escape(ui);
        let window = self.show_window(ui, &theme);
        let action = window.chrome.or(escape).or(window.screen);

        let ctx = ui.ctx().clone();
        self.onboarding.ui(&ctx, &mut self.services, &theme);

        self.services.toasts.show(&ctx, &theme);
        focus::end_frame(&ctx, &theme);
        platform::end_frame(&ctx);

        self.take_pending_play(&ctx);
        if let Some(action) = action {
            self.apply_nav(action, ui.input(|i| i.time), &ctx);
        }

        self.services.images.end_frame();
    }
}
