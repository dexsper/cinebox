//! First-run wizard: language, TMDB key, parser, TorrServer. Every step can
//! be skipped; the settings drawer can start the wizard again.

mod card;
mod steps;

use std::borrow::Cow;

use egui::{Align, Context, Layout, RichText, Ui, vec2};
use egui_async::Bind;
use egui_material_icons::icons::{ICON_ARROW_BACK, ICON_ARROW_FORWARD, ICON_CLOSE};
use rust_i18n::t;

use crate::discovery::{Discovery, DiscoveryCtx, discover_services};
use crate::jobs::JobError;
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
    Tmdb,
    Parser,
    TorrServer,
    Done,
}

impl Step {
    const ALL: [Self; 5] = [
        Self::Language,
        Self::Tmdb,
        Self::Parser,
        Self::TorrServer,
        Self::Done,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|step| *step == self).unwrap_or(0)
    }

    fn next(self) -> Self {
        let next = Self::ALL.get(self.index() + 1);
        next.copied().unwrap_or(self)
    }

    fn prev(self) -> Self {
        let Some(index) = self.index().checked_sub(1) else {
            return self;
        };

        Self::ALL[index]
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

        if let Some(nav) = nav {
            self.apply(nav, svc);
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
        match self.step {
            Step::Language => steps::language(ui, svc, theme),
            Step::Tmdb => self.tmdb_step(ui, svc, theme),
            Step::Parser => self.parser_step(ui, svc, theme),
            Step::TorrServer => self.torr_step(ui, svc, theme),
            Step::Done => self.done_step(ui, svc, theme),
        }
    }

    /// Step counter on the left, "Set up later" on the right.
    fn header(&self, ui: &mut Ui, theme: &Theme) -> Option<WizardNav> {
        let number = (self.step.index() + 1).to_string();
        let total = Step::ALL.len().to_string();
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

    fn apply(&mut self, nav: WizardNav, svc: &mut Services) {
        match nav {
            WizardNav::Back => self.step = self.step.prev(),
            WizardNav::Next => self.step = self.step.next(),
            WizardNav::Finish => self.finish(svc),
        }
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
        assert_eq!(Step::Language.prev(), Step::Language);
        assert_eq!(Step::Language.next(), Step::Tmdb);
        assert_eq!(Step::TorrServer.next(), Step::Done);
        assert_eq!(Step::Done.next(), Step::Done);
        assert_eq!(Step::Done.prev(), Step::TorrServer);
    }
}
