//! Bodies of the wizard steps and the small pieces they share.

use cinebox_core::{ParserKind, SecretString, Settings, UiLanguage};
use egui::{Color32, Id, OpenUrl, RichText, Ui};
use egui_async::Bind;
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_CHECK_CIRCLE, ICON_ERROR, ICON_HELP, ICON_OPEN_IN_NEW, ICON_REFRESH, ICON_REMOVE,
};
use rust_i18n::t;

use super::{OnboardingScreen, Step, card};
use crate::discovery::{Discovery, FoundParser};
use crate::errors::UserError;
use crate::jobs::{self, JobError};
use crate::screens::gate::{self, TmdbKeyProblem};
use crate::services::Services;
use crate::settings_input::{InputKind, changed_value};
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::field;

const LANGUAGES: [(UiLanguage, &str); 3] = [
    (UiLanguage::English, "English"),
    (UiLanguage::Russian, "Русский"),
    (UiLanguage::Ukrainian, "Українська"),
];

const LINK_H: f32 = 30.0;

/// Whether the step's service has a value, so "Next" does not read as "Skip".
pub(super) fn is_set(step: Step, settings: &Settings) -> bool {
    match step {
        Step::Language | Step::Done => true,
        Step::Tmdb => !settings.tmdb.api_key.is_empty(),
        Step::Parser => !settings.parser.url.is_empty(),
        Step::TorrServer => !settings.torrserver.url.is_empty(),
    }
}

/// Applied at once, so the rest of the wizard already speaks the language.
pub(super) fn language(ui: &mut Ui, svc: &mut Services, theme: &Theme) {
    ui.spacing_mut().item_spacing.y = 8.0;
    for (lang, name) in LANGUAGES {
        let selected = svc.settings.general.language == lang;
        if !card::choice(ui, theme, name, None, selected) {
            continue;
        }

        svc.settings.general.language = lang;
        crate::i18n::apply(lang);
        svc.persist();
    }
}

impl OnboardingScreen {
    pub(super) fn tmdb_step(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        if link(ui, theme, ICON_OPEN_IN_NEW, &t!("gate.tmdb_get_key")) {
            ui.ctx().open_url(OpenUrl::new_tab(gate::TMDB_KEY_URL));
        }

        ui.add_space(12.0);
        let current = svc.settings.tmdb.api_key.expose().to_owned();
        let label = t!("settings.api_key");
        let spec = Input::plain(&label, "tmdb-key");
        let draft = edit(ui, theme, &spec, &current);
        if let Some(key) = changed_value(draft, InputKind::Key, &current) {
            svc.settings.tmdb.api_key = SecretString::from(key);
            svc.persist();
            self.check_tmdb(svc);
        }

        if let Some(problem) = gate::tmdb_key_problem(&svc.settings) {
            key_problem(ui, theme, problem);
        }

        probe_status(ui, theme, &mut self.probes.tmdb);
    }

    pub(super) fn parser_step(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        self.found_header(ui, svc, theme);
        if let Some(found) = found_parser(ui, theme, &mut self.discovery, svc) {
            self.use_parser(found, svc);
        }

        ui.add_space(16.0);
        section(ui, theme, &t!("wizard.manual"));
        if parser_kind_row(ui, theme, svc) {
            self.check_parser(svc);
        }

        ui.add_space(8.0);
        let url = svc.settings.parser.url.clone();
        let label = t!("settings.url");
        let spec = Input::url(&label, "parser-url", "http://127.0.0.1:9117");
        let draft = edit(ui, theme, &spec, &url);
        if let Some(url) = changed_value(draft, InputKind::Url, &url) {
            svc.settings.parser.url = url;
            svc.persist();
            self.check_parser(svc);
        }

        let key = svc.settings.parser.api_key.expose().to_owned();
        let label = t!("settings.api_key");
        let spec = Input::plain(&label, "parser-key");
        let draft = edit(ui, theme, &spec, &key);
        if let Some(key) = changed_value(draft, InputKind::Key, &key) {
            svc.settings.parser.api_key = SecretString::from(key);
            svc.persist();
            self.check_parser(svc);
        }

        probe_status(ui, theme, &mut self.probes.parser);
    }

    pub(super) fn torr_step(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        self.found_header(ui, svc, theme);
        if let Some(url) = found_torrserver(ui, theme, &mut self.discovery, svc) {
            svc.settings.torrserver.url = url;
            svc.persist();
            self.check_torr(svc);
        }

        ui.add_space(16.0);
        section(ui, theme, &t!("wizard.manual"));
        let url = svc.settings.torrserver.url.clone();
        let label = t!("settings.url");
        let spec = Input::url(&label, "torr-url", "http://127.0.0.1:8090");
        let draft = edit(ui, theme, &spec, &url);
        if let Some(url) = changed_value(draft, InputKind::Url, &url) {
            svc.settings.torrserver.url = url;
            svc.persist();
            self.check_torr(svc);
        }

        let auth_label = t!("wizard.torr_auth");
        let auth = Opts::chip(self.show_torr_auth);
        if button::label(ui, theme, &auth_label, auth) {
            self.show_torr_auth = !self.show_torr_auth;
        }

        if self.show_torr_auth {
            self.torr_auth(ui, svc, theme);
        }

        probe_status(ui, theme, &mut self.probes.torr);
    }

    fn torr_auth(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        let user = svc.settings.torrserver.username.clone();
        let label = t!("settings.username");
        let spec = Input::plain(&label, "torr-user");
        let draft = edit(ui, theme, &spec, &user);
        if let Some(user) = changed_value(draft, InputKind::Plain, &user) {
            svc.settings.torrserver.username = user;
            svc.persist();
            self.check_torr(svc);
        }

        let password = svc.settings.torrserver.password.expose().to_owned();
        let label = t!("settings.password");
        let spec = Input::password(&label, "torr-password");
        let draft = edit(ui, theme, &spec, &password);
        if let Some(password) = changed_value(draft, InputKind::Raw, &password) {
            svc.settings.torrserver.password = SecretString::from(password);
            svc.persist();
            self.check_torr(svc);
        }
    }

    pub(super) fn done_step(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme) {
        let settings = &svc.settings;
        let tmdb = readiness(is_set(Step::Tmdb, settings), &mut self.probes.tmdb);
        let parser = readiness(is_set(Step::Parser, settings), &mut self.probes.parser);
        let torr = readiness(is_set(Step::TorrServer, settings), &mut self.probes.torr);

        ui.spacing_mut().item_spacing.y = 10.0;
        summary_row(ui, theme, "TMDB", tmdb);
        summary_row(ui, theme, &t!("settings.parser"), parser);
        summary_row(ui, theme, "TorrServer", torr);
    }

    /// "Found nearby" with a button to search again.
    fn found_header(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme) {
        let mut again = false;
        ui.horizontal(|ui| {
            section(ui, theme, &t!("wizard.found"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                again = link(ui, theme, ICON_REFRESH, &t!("wizard.search_again"));
            });
        });

        if again && !self.discovery.is_pending() {
            self.search_services(svc);
        }
    }

    fn use_parser(&mut self, found: FoundParser, svc: &mut Services) {
        let parser = &mut svc.settings.parser;
        parser.kind = found.kind;
        parser.url = found.url;
        if found.public {
            parser.api_key = SecretString::default();
        }

        svc.persist();
        self.check_parser(svc);
    }

    /// Check whatever is already filled in, so a re-run starts with real status.
    pub(super) fn check_configured(&mut self, svc: &Services) {
        let settings = &svc.settings;
        if is_set(Step::Tmdb, settings) {
            self.check_tmdb(svc);
        }

        if is_set(Step::Parser, settings) {
            self.check_parser(svc);
        }

        if is_set(Step::TorrServer, settings) {
            self.check_torr(svc);
        }
    }

    fn check_tmdb(&mut self, svc: &Services) {
        if gate::tmdb_key_problem(&svc.settings).is_some() {
            self.probes.tmdb.clear();
            return;
        }

        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        restart(&mut self.probes.tmdb, jobs::ping_tmdb(tmdb, svc.db.clone()));
    }

    fn check_parser(&mut self, svc: &Services) {
        if !is_set(Step::Parser, &svc.settings) {
            self.probes.parser.clear();
            return;
        }

        let parser = jobs::ParserCtx::from(&svc.settings);
        restart(&mut self.probes.parser, jobs::ping_parser(parser));
    }

    fn check_torr(&mut self, svc: &Services) {
        if !is_set(Step::TorrServer, &svc.settings) {
            self.probes.torr.clear();
            return;
        }

        let torr = jobs::TorrCtx::from(&svc.settings);
        restart(&mut self.probes.torr, jobs::ping_torrserver(torr));
    }
}

fn restart<F>(bind: &mut Bind<String, JobError>, job: F)
where
    F: Future<Output = Result<String, JobError>> + Send + 'static,
{
    bind.clear();
    bind.request(job);
}

/// Discovery results once ready; draws "searching" or "nothing" otherwise.
fn discovered<'a>(ui: &mut Ui, theme: &Theme, bind: &'a mut Bind<Discovery, JobError>) -> Option<&'a Discovery> {
    if bind.is_pending() {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().color(theme.muted));
            ui.label(small(theme, t!("wizard.searching"), theme.muted));
        });
        return None;
    }

    let Some(Ok(discovery)) = bind.read() else {
        return None;
    };

    Some(discovery)
}

fn found_parser(
    ui: &mut Ui,
    theme: &Theme,
    bind: &mut Bind<Discovery, JobError>,
    svc: &Services,
) -> Option<FoundParser> {
    let discovery = discovered(ui, theme, bind)?;
    if discovery.parsers.is_empty() {
        ui.label(small(theme, t!("wizard.found_nothing"), theme.muted));
        return None;
    }

    let mut picked = None;
    ui.spacing_mut().item_spacing.y = 8.0;
    for parser in &discovery.parsers {
        let selected = svc.settings.parser.url == parser.url;
        let title = format!("{} · {}", parser.kind, host(&parser.url));
        let note = parser_note(parser);
        if card::choice(ui, theme, &title, Some(&note), selected) {
            picked = Some(parser.clone());
        }
    }

    picked
}

fn parser_note(parser: &FoundParser) -> String {
    let ms = parser.latency_ms.to_string();
    let latency = t!("wizard.latency", ms = ms);
    if parser.needs_key {
        return format!("{} · {latency}", t!("wizard.needs_key"));
    }

    if parser.public {
        return format!("{} · {latency}", t!("wizard.public"));
    }

    latency.into_owned()
}

fn found_torrserver(
    ui: &mut Ui,
    theme: &Theme,
    bind: &mut Bind<Discovery, JobError>,
    svc: &Services,
) -> Option<String> {
    let discovery = discovered(ui, theme, bind)?;
    if discovery.torrservers.is_empty() {
        ui.label(small(theme, t!("wizard.found_nothing"), theme.muted));
        return None;
    }

    let mut picked = None;
    ui.spacing_mut().item_spacing.y = 8.0;
    for server in &discovery.torrservers {
        let selected = svc.settings.torrserver.url == server.url;
        let title = format!("TorrServer {}", server.version);
        if card::choice(ui, theme, &title, Some(host(&server.url)), selected) {
            picked = Some(server.url.clone());
        }
    }

    picked
}

/// Jackett / Prowlarr chips. `true` when the kind changed.
fn parser_kind_row(ui: &mut Ui, theme: &Theme, svc: &mut Services) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for kind in ParserKind::ALL {
            let selected = svc.settings.parser.kind == *kind;
            let opts = Opts::chip(selected);
            if button::label(ui, theme, &kind.to_string(), opts) && !selected {
                svc.settings.parser.kind = *kind;
                changed = true;
            }
        }
    });

    if changed {
        svc.persist();
    }

    changed
}

fn key_problem(ui: &mut Ui, theme: &Theme, problem: TmdbKeyProblem) {
    let hint = match problem {
        TmdbKeyProblem::Missing => return,
        TmdbKeyProblem::AccessToken => t!("settings.tmdb_key_token"),
        TmdbKeyProblem::BadFormat => t!("settings.tmdb_key_format"),
    };

    ui.label(small(theme, hint, theme.err));
}

fn probe_status(ui: &mut Ui, theme: &Theme, bind: &mut Bind<String, JobError>) {
    if bind.is_pending() {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().color(theme.muted));
            ui.label(small(theme, t!("wizard.checking"), theme.muted));
        });
        return;
    }

    match bind.read() {
        None => {}
        Some(Ok(_)) => {
            ui.label(small(theme, t!("wizard.connected"), theme.ok));
        }
        Some(Err(error)) => {
            let summary = UserError::from(error).summary();
            ui.label(small(theme, summary, theme.err));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Readiness {
    Working,
    Failing,
    Unchecked,
    Missing,
}

fn readiness(set: bool, probe: &mut Bind<String, JobError>) -> Readiness {
    if !set {
        return Readiness::Missing;
    }

    match probe.read() {
        Some(Ok(_)) => Readiness::Working,
        Some(Err(_)) => Readiness::Failing,
        None => Readiness::Unchecked,
    }
}

fn summary_row(ui: &mut Ui, theme: &Theme, name: &str, readiness: Readiness) {
    let (icon, color, status) = match readiness {
        Readiness::Working => (ICON_CHECK_CIRCLE, theme.ok, t!("wizard.working")),
        Readiness::Failing => (ICON_ERROR, theme.err, t!("wizard.failing")),
        Readiness::Unchecked => (ICON_HELP, theme.muted, t!("wizard.unchecked")),
        Readiness::Missing => (ICON_REMOVE, theme.muted, t!("wizard.missing")),
    };

    ui.horizontal(|ui| {
        ui.label(icon_text(theme, icon, color));
        ui.label(RichText::new(name).size(theme.text_body).color(theme.title));
        ui.label(small(theme, status, theme.muted));
    });
}

/// A text input with its label, remembered by `id` between frames.
struct Input<'a> {
    label: &'a str,
    id: &'static str,
    placeholder: &'static str,
    password: bool,
}

impl<'a> Input<'a> {
    fn plain(label: &'a str, id: &'static str) -> Self {
        Self {
            label,
            id,
            placeholder: "",
            password: false,
        }
    }

    fn url(label: &'a str, id: &'static str, placeholder: &'static str) -> Self {
        Self {
            placeholder,
            ..Self::plain(label, id)
        }
    }

    fn password(label: &'a str, id: &'static str) -> Self {
        Self {
            password: true,
            ..Self::plain(label, id)
        }
    }
}

/// Returns the raw draft once it commits (see [`field::committed_edit`]).
fn edit(ui: &mut Ui, theme: &Theme, input: &Input<'_>, value: &str) -> Option<String> {
    ui.label(small(theme, input.label, theme.muted_bright));
    let id = Id::new(("wizard", input.id));
    let placeholder = input.placeholder;

    field::committed_edit(ui, theme, id, value, placeholder, input.password)
}

fn section(ui: &mut Ui, theme: &Theme, text: &str) {
    let font = theme.title_font(theme.text_section);
    ui.label(RichText::new(text).font(font).color(theme.title));
}

fn link(ui: &mut Ui, theme: &Theme, icon: MaterialIcon, label: &str) -> bool {
    let opts = Opts::secondary(egui::vec2(0.0, LINK_H));
    button::icon_label(ui, theme, icon, label, opts)
}

fn small(theme: &Theme, text: impl Into<String>, color: Color32) -> RichText {
    RichText::new(text).size(theme.text_small).color(color)
}

fn icon_text(theme: &Theme, icon: MaterialIcon, color: Color32) -> RichText {
    icon.rich_text().size(theme.text_icon_md).color(color)
}

/// `https://jac.red` → `jac.red`.
fn host(url: &str) -> &str {
    let without_scheme = url.split("://").nth(1);
    without_scheme.unwrap_or(url)
}
