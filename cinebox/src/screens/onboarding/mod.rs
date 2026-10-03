//! First-run wizard: language, fitting a TV's screen, TMDB key, parser,
//! TorrServer. Every step can be skipped; the settings drawer can start the
//! wizard again.

mod card;
mod steps;

use std::borrow::Cow;

use egui::{Align, Context, Layout, RichText, Ui, vec2};
use egui_async::Bind;
use egui_material_icons::icons::{ICON_ARROW_BACK, ICON_ARROW_FORWARD, ICON_CLOSE};
use rust_i18n::t;

use crate::discovery::{Discovery, DiscoveryCtx, discover_services};
use crate::jobs::JobError;
use crate::platform;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::{focus, intro};

const FOOTER_BUTTON: egui::Vec2 = vec2(132.0, 36.0);
/// Header, title, and footer around the scrolling step body.
const CARD_CHROME_H: f32 = 220.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Language,
    Screen,
    Tmdb,
    Parser,
    TorrServer,
    Done,
}

/// The steps this device walks through. Where the screen may crop the
/// picture, it is fitted right after the language, before anything else is
/// laid out against its edges.
#[derive(Clone, Copy)]
struct Route(&'static [Step]);

impl Route {
    const CROPPING_SCREEN: [Step; 6] = [
        Step::Language,
        Step::Screen,
        Step::Tmdb,
        Step::Parser,
        Step::TorrServer,
        Step::Done,
    ];

    const WHOLE_SCREEN: [Step; 5] = [
        Step::Language,
        Step::Tmdb,
        Step::Parser,
        Step::TorrServer,
        Step::Done,
    ];

    fn of(ctx: &Context) -> Self {
        if platform::profile(ctx).overscan {
            return Self(&Self::CROPPING_SCREEN);
        }

        Self(&Self::WHOLE_SCREEN)
    }

    fn len(self) -> usize {
        self.0.len()
    }

    fn index(self, step: Step) -> usize {
        self.0.iter().position(|item| *item == step).unwrap_or(0)
    }

    fn next(self, step: Step) -> Step {
        let next = self.0.get(self.index(step) + 1);
        next.copied().unwrap_or(step)
    }

    fn prev(self, step: Step) -> Step {
        let Some(index) = self.index(step).checked_sub(1) else {
            return step;
        };

        self.0[index]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WizardNav {
    Back,
    Next,
    Finish,
}

/// One connection check per service, started when its address or key commits.
struct Probes {
    tmdb: Bind<String, JobError>,
    parser: Bind<String, JobError>,
    torr: Bind<String, JobError>,
}

impl Default for Probes {
    fn default() -> Self {
        Self {
            tmdb: Bind::new(true),
            parser: Bind::new(true),
            torr: Bind::new(true),
        }
    }
}

pub struct OnboardingScreen {
    open: bool,
    opened_at: Option<f64>,
    step: Step,
    discovery: Bind<Discovery, JobError>,
    probes: Probes,
    show_torr_auth: bool,
    /// The step changed since the last frame.
    arrived: bool,
}

impl Default for OnboardingScreen {
    fn default() -> Self {
        Self {
            open: false,
            opened_at: None,
            step: Step::Language,
            discovery: Bind::new(true),
            probes: Probes::default(),
            show_torr_auth: false,
            arrived: false,
        }
    }
}

impl OnboardingScreen {
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Start from the first step and look for local services in the background.
    pub fn open(&mut self, svc: &Services, now: f64) {
        *self = Self::default();
        self.open = true;
        self.opened_at = Some(now);
        self.show_torr_auth = !svc.settings.torrserver.username.is_empty();

        self.search_services(svc);
        self.check_configured(svc);
    }

    fn search_services(&mut self, svc: &Services) {
        let job = discover_services(discovery_ctx(svc));
        self.discovery.clear();
        self.discovery.request(job);
    }

    /// Open without touching the network, fully faded in.
    #[cfg(test)]
    pub(crate) fn open_offline(&mut self) {
        *self = Self::default();
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &Context, svc: &mut Services, theme: &Theme) {
        if !self.open {
            return;
        }

        let now = ctx.input(|i| i.time);
        let t = intro::t(self.opened_at, now);
        if intro::running(self.opened_at, now) {
            ctx.request_repaint();
        }

        let max_body_h = card::max_body_height(ctx, theme, CARD_CHROME_H);
        let mut nav = None;
        card::show(ctx, theme, t, |ui| {
            nav = self.card_contents(ui, svc, theme, max_body_h);
        });

        if self.step == Step::Screen {
            crate::widgets::overscan::corner_marks(ctx, theme);
        }

        if let Some(nav) = nav {
            self.apply(nav, svc, Route::of(ctx));
        }
    }

    fn card_contents(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        max_body_h: f32,
    ) -> Option<WizardNav> {
        let later = self.header(ui, theme);
        ui.add_space(16.0);
        self.title(ui, theme);
        ui.add_space(20.0);

        egui::ScrollArea::vertical()
            .max_height(max_body_h)
            .auto_shrink([false, true])
            .show(ui, |ui| self.step_body(ui, svc, theme));

        ui.add_space(24.0);
        let footer = self.footer(ui, svc, theme);
        later.or(footer)
    }

    fn step_body(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        let arrived = std::mem::take(&mut self.arrived);
        match self.step {
            Step::Language => steps::language(ui, svc, theme),
            Step::Screen => steps::screen(ui, svc, theme, arrived),
            Step::Tmdb => self.tmdb_step(ui, svc, theme),
            Step::Parser => self.parser_step(ui, svc, theme),
            Step::TorrServer => self.torr_step(ui, svc, theme),
            Step::Done => self.done_step(ui, svc, theme),
        }
    }

    /// Step counter on the left, "Set up later" on the right.
    fn header(&self, ui: &mut Ui, theme: &Theme) -> Option<WizardNav> {
        let route = Route::of(ui.ctx());
        let number = (route.index(self.step) + 1).to_string();
        let total = route.len().to_string();
        let counter = t!("wizard.step_of", number = number, total = total);

        let mut later = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(counter).size(theme.text_small).color(theme.muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = t!("wizard.later");
                let opts = Opts::secondary(vec2(0.0, 28.0));
                later = button::icon_label(ui, theme, ICON_CLOSE, &label, opts);
            });
        });

        later.then_some(WizardNav::Finish)
    }

    fn title(&self, ui: &mut Ui, theme: &Theme) {
        let (title, subtitle) = step_copy(self.step);
        let title_font = theme.title_font(theme.text_display);
        ui.label(RichText::new(title).font(title_font).color(theme.title));
        ui.add_space(6.0);
        ui.label(RichText::new(subtitle).size(theme.text_body).color(theme.muted_bright));
    }

    fn footer(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme) -> Option<WizardNav> {
        let mut nav = None;
        ui.horizontal(|ui| {
            if self.step != Step::Language {
                let back = t!("wizard.back");
                let opts = Opts::secondary(FOOTER_BUTTON);
                if button::icon_label(ui, theme, ICON_ARROW_BACK, &back, opts) {
                    nav = Some(WizardNav::Back);
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = self.next_label(svc);
                let opts = Opts::primary(FOOTER_BUTTON);
                let next = button::icon_label_response(ui, theme, ICON_ARROW_FORWARD, &label, opts);
                // Unless the step marked its current choice, OK keeps moving forward.
                focus::prefer(&next);
                if next.clicked() {
                    nav = Some(self.forward());
                }
            });
        });

        nav
    }

    fn forward(&self) -> WizardNav {
        if self.step == Step::Done {
            return WizardNav::Finish;
        }

        WizardNav::Next
    }

    /// "Skip" while the step's service is not set, so moving on never looks
    /// like it saved something.
    fn next_label(&self, svc: &Services) -> Cow<'static, str> {
        if self.step == Step::Done {
            return t!("wizard.start");
        }

        if steps::is_set(self.step, &svc.settings) {
            return t!("wizard.next");
        }

        t!("wizard.skip")
    }

    fn apply(&mut self, nav: WizardNav, svc: &mut Services, route: Route) {
        match nav {
            WizardNav::Back => self.go_to(route.prev(self.step)),
            WizardNav::Next => self.go_to(route.next(self.step)),
            WizardNav::Finish => self.finish(svc),
        }
    }

    fn go_to(&mut self, step: Step) {
        self.arrived = step != self.step;
        self.step = step;
    }

    /// Closing at any step counts: the wizard does not come back on its own.
    fn finish(&mut self, svc: &mut Services) {
        self.open = false;
        svc.settings.general.onboarded = true;
        svc.persist();
    }
}

fn step_copy(step: Step) -> (Cow<'static, str>, Cow<'static, str>) {
    match step {
        Step::Language => (t!("wizard.language_title"), t!("wizard.language_body")),
        Step::Screen => (t!("wizard.screen_title"), t!("wizard.screen_body")),
        Step::Tmdb => (t!("wizard.tmdb_title"), t!("wizard.tmdb_body")),
        Step::Parser => (t!("wizard.parser_title"), t!("wizard.parser_body")),
        Step::TorrServer => (t!("wizard.torr_title"), t!("wizard.torr_body")),
        Step::Done => (t!("wizard.done_title"), t!("wizard.done_body")),
    }
}

fn discovery_ctx(svc: &Services) -> DiscoveryCtx {
    let settings_file = svc.store.as_ref().map(|store| store.path());
    let config_dir = settings_file.and_then(std::path::Path::parent);

    DiscoveryCtx {
        net: crate::jobs::net_config(&svc.settings),
        db: svc.db.clone(),
        config_dir: config_dir.map(std::path::Path::to_path_buf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_walk_forward_and_back_within_bounds() {
        let route = Route(&Route::WHOLE_SCREEN);
        assert_eq!(route.prev(Step::Language), Step::Language);
        assert_eq!(route.next(Step::Language), Step::Tmdb);
        assert_eq!(route.next(Step::TorrServer), Step::Done);
        assert_eq!(route.next(Step::Done), Step::Done);
        assert_eq!(route.prev(Step::Done), Step::TorrServer);
    }

    #[test]
    fn a_cropping_screen_is_fitted_right_after_the_language() {
        let route = Route(&Route::CROPPING_SCREEN);
        assert_eq!(route.next(Step::Language), Step::Screen);
        assert_eq!(route.next(Step::Screen), Step::Tmdb);
        assert_eq!(route.prev(Step::Tmdb), Step::Screen);
    }
}
