//! Settings as a right-hand overlay drawer. Fields come from [`catalog`].

mod catalog;
mod controls;
mod speed;

use cinebox_core::{HomeRowId, ParserKind, PosterSize, QualityBand, SecretString, UiLanguage};
use egui::Ui;
use egui_async::Bind;
use egui_material_icons::icons::{ICON_KEY, ICON_NETWORK_PING, ICON_SEARCH};
use rust_i18n::t;

use crate::app::nav::{NavAction, SettingsPage};
use crate::services::Services;
use crate::services::jobs::{self, JobError};
use crate::services::settings_input::changed_value;
use crate::theme::Theme;
use crate::widgets::combo::Choices;
use crate::widgets::drawer::Overlay;
use crate::widgets::{focus, scroll};
use catalog::{CategoryId, Check, Field, MultiSelectId, SelectId, catalog, category};
use controls::{
    Action, Labeled, TextRow, category_row, clear_cache_row, drawer_title, error_line, field_warning,
    multiselect_chip_row, nav_header, probe_row, secret_row, select_row, select_row_with, speed_test_row, text_row,
    toggle_row, visibility_chip_row, wizard_row,
};
use speed::SpeedMeter;

pub(crate) struct SettingsScreen {
    overlay: Overlay,
    category: Option<CategoryId>,
    /// The category just left for the list, whose row takes the D-pad back.
    left_category: Option<CategoryId>,
    torr: Bind<String, JobError>,
    parser: Bind<String, JobError>,
    tmdb: Bind<String, JobError>,
    speed: Bind<(), JobError>,
    speed_meter: SpeedMeter,
    wizard_requested: bool,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self {
            overlay: Overlay::default(),
            category: None,
            left_category: None,
            torr: Bind::new(true),
            parser: Bind::new(true),
            tmdb: Bind::new(true),
            speed: Bind::new(true),
            speed_meter: SpeedMeter::new(),
            wizard_requested: false,
        }
    }
}

impl SettingsScreen {
    pub(crate) fn is_open(&self) -> bool {
        self.overlay.is_open()
    }

    pub(crate) fn toggle(&mut self, now: f64) {
        self.overlay.toggle(now);
    }

    pub(crate) fn close(&mut self, now: f64) {
        if self.overlay.is_open() {
            self.overlay.begin_close(now);
        }
    }

    /// Open the drawer (if closed) straight on `page`.
    pub(crate) fn open_at(&mut self, page: SettingsPage, now: f64) {
        if !self.overlay.is_open() {
            self.overlay.begin_open(now);
        }

        self.category = Some(CategoryId::from(page));
    }

    /// Consume Escape / chrome Back. `true` if the drawer handled it.
    pub(crate) fn on_back(&mut self, now: f64) -> bool {
        if !self.overlay.is_blocking(now) {
            return false;
        }

        if !self.overlay.is_open() {
            return true;
        }

        if self.category.is_some() {
            self.leave_category();
            return true;
        }

        self.overlay.begin_close(now);
        true
    }

    /// Returns navigation the drawer asks for (the setup wizard).
    pub(crate) fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) -> Option<NavAction> {
        let now = ui.input(|i| i.time);
        if !self.overlay.is_blocking(now) {
            self.category = None;
            self.left_category = None;
        }

        let mut overlay = std::mem::take(&mut self.overlay);
        overlay.paint(ui, theme, "cinebox-settings", |ui, theme| {
            self.paint_body(ui, svc, theme);
        });
        self.overlay = overlay;

        let wizard = std::mem::take(&mut self.wizard_requested);
        wizard.then_some(NavAction::OpenOnboarding)
    }

    fn leave_category(&mut self) {
        self.left_category = self.category.take();
    }

    fn paint_body(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        if let Some(id) = self.category {
            self.paint_category(ui, svc, theme, id);
            return;
        }

        self.paint_list(ui, svc, theme);
    }

    fn paint_list(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme) {
        drawer_title(ui, theme, t!("settings.title").as_ref());
        ui.add_space(4.0);
        paint_errors(ui, svc, theme);
        ui.add_space(12.0);

        scroll::vertical(ui, "settings-categories", |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            if wizard_row(ui, theme) {
                self.wizard_requested = true;
            }

            for cat in catalog() {
                let row = category_row(ui, theme, cat);
                if self.left_category == Some(cat.id) {
                    focus::prefer(&row);
                }

                if row.clicked() {
                    self.category = Some(cat.id);
                }
            }
        });
    }

    fn paint_category(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme, id: CategoryId) {
        let cat = category(id);
        if nav_header(ui, theme, crate::i18n::tr(cat.title).as_ref()) {
            self.leave_category();
            return;
        }

        ui.add_space(4.0);
        paint_errors(ui, svc, theme);
        ui.add_space(8.0);

        let mut persist = false;
        scroll::vertical(ui, ("settings-fields", cat.id.as_key()), |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            for field in cat.fields {
                persist |= self.paint_field(ui, svc, theme, field);
            }
        });

        if persist {
            svc.persist();
        }
    }

    fn paint_field(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme, field: &Field) -> bool {
        match field {
            Field::Toggle { label, hint, get, set } => {
                let mut value = get(&svc.settings);
                let label_text = crate::i18n::tr(label);
                let hint_text = hint.map(|key| crate::i18n::tr(key));
                if !toggle_row(ui, theme, label_text.as_ref(), hint_text.as_deref(), &mut value) {
                    return false;
                }
                set(&mut svc.settings, value);
                true
            }
            Field::Text { label, hint, placeholder, kind, check, get, set } => {
                let value = get(&svc.settings);
                let label_text = crate::i18n::tr(label);
                let hint_text = hint.map(|key| crate::i18n::tr(key));
                let row = TextRow {
                    label: label_text.as_ref(),
                    hint: hint_text.as_deref(),
                    placeholder,
                    purpose: kind.text_purpose(),
                };
                let draft = text_row(ui, theme, row, &value);
                show_check(ui, theme, *check, &value);

                let Some(next) = changed_value(draft, *kind, &value) else {
                    return false;
                };
                set(&mut svc.settings, next);
                true
            }
            Field::Secret { label, hint, kind, check, get, set } => {
                let value = get(&svc.settings);
                let label_text = crate::i18n::tr(label);
                let hint_text = hint.map(|key| crate::i18n::tr(key));
                let draft = secret_row(ui, theme, label_text.as_ref(), hint_text.as_deref(), &value);
                show_check(ui, theme, *check, value.expose());

                let Some(next) = changed_value(draft, *kind, value.expose()) else {
                    return false;
                };
                set(&mut svc.settings, SecretString::from(next));
                true
            }
            Field::Select { id, label, hint, which } => {
                let label_text = crate::i18n::tr(label);
                let hint_text = hint.map(|key| crate::i18n::tr(key));
                let labeled = Labeled { id, label: label_text.as_ref(), hint: hint_text.as_deref() };
                paint_select(ui, svc, theme, labeled, which)
            }
            Field::MultiSelect { id, label, hint, which } => {
                let label_text = crate::i18n::tr(label);
                let hint_text = hint.map(|key| crate::i18n::tr(key));
                let labeled = Labeled { id, label: label_text.as_ref(), hint: hint_text.as_deref() };
                paint_multiselect(ui, svc, theme, labeled, which)
            }
            Field::PlayerOnly { needs, field } => {
                if !needs(&svc.player_features()) {
                    return false;
                }

                self.paint_field(ui, svc, theme, field)
            }
            Field::ProbeParser => {
                let parser = jobs::ParserCtx::from(&svc.settings);
                let label = t!("settings.test_parser");
                let action = Action { icon: ICON_SEARCH, label: label.as_ref() };
                probe_row(ui, theme, action, &mut self.parser, || jobs::ping_parser(parser));
                false
            }
            Field::ProbeTorr => {
                let torr = jobs::TorrCtx::from(&svc.settings);
                let label = t!("settings.ping");
                let action = Action { icon: ICON_NETWORK_PING, label: label.as_ref() };
                probe_row(ui, theme, action, &mut self.torr, || jobs::ping_torrserver(torr));
                false
            }
            Field::ProbeTmdb => {
                let tmdb = jobs::TmdbCtx::from(&svc.settings);
                let db = svc.db.clone();
                let label = t!("settings.check_api_key");
                let action = Action { icon: ICON_KEY, label: label.as_ref() };
                probe_row(ui, theme, action, &mut self.tmdb, || jobs::ping_tmdb(tmdb, db));
                false
            }
            Field::SpeedTest => {
                let torr = jobs::TorrCtx::from(&svc.settings);
                let meter = self.speed_meter.clone();
                let ctx = ui.ctx().clone();
                speed_test_row(ui, theme, &self.speed_meter, &mut self.speed, move || speed::run(torr, meter, ctx));
                false
            }
            Field::ClearCache => {
                if clear_cache_row(ui, theme) {
                    svc.clear_tmdb_cache();
                }
                false
            }
        }
    }
}

fn show_check(ui: &mut Ui, theme: &Theme, check: Option<Check>, value: &str) {
    let Some(key) = check.and_then(|check| check(value)) else {
        return;
    };

    field_warning(ui, theme, crate::i18n::tr(key).as_ref());
}

fn paint_select(ui: &mut Ui, svc: &mut Services, theme: &Theme, field: Labeled<'_>, which: &SelectId) -> bool {
    match which {
        SelectId::Language => {
            let choices = Choices { options: UiLanguage::ALL, label: |lang| ui_lang_label(lang).into_owned() };
            select_row_with(ui, theme, field, &mut svc.settings.general.language, choices)
        }
        SelectId::ParserKind => select_row(ui, theme, field, &mut svc.settings.parser.kind, ParserKind::ALL),
        SelectId::PosterSize => select_row(ui, theme, field, &mut svc.settings.tmdb.poster_size, PosterSize::ALL),
    }
}

fn paint_multiselect(
    ui: &mut Ui,
    svc: &mut Services,
    theme: &Theme,
    field: Labeled<'_>,
    which: &MultiSelectId,
) -> bool {
    match which {
        MultiSelectId::Quality => {
            let choices = Choices { options: QualityBand::ALL, label: |band: QualityBand| band.label().to_owned() };
            multiselect_chip_row(ui, theme, field, &mut svc.settings.parser.default_quality, choices)
        }
        MultiSelectId::HomeRows => {
            let label = |row| crate::i18n::home_row_title(row).into_owned();
            let choices = Choices { options: &HomeRowId::ALL, label };
            visibility_chip_row(ui, theme, field, &mut svc.settings.general.hidden_home_rows, choices)
        }
    }
}

fn ui_lang_label(lang: UiLanguage) -> std::borrow::Cow<'static, str> {
    match lang {
        UiLanguage::English => t!("lang.en"),
        UiLanguage::Russian => t!("lang.ru"),
        UiLanguage::Ukrainian => t!("lang.uk"),
    }
}

fn paint_errors(ui: &mut Ui, svc: &Services, theme: &Theme) {
    if let Some(error) = &svc.load_error {
        error_line(ui, theme, t!("settings.load_error").as_ref());
        error_line(ui, theme, error);
    }
    if let Some(error) = &svc.save_error {
        error_line(ui, theme, &format!("{} {error}", t!("settings.could_not_save")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_closed() {
        let screen = SettingsScreen::default();

        assert!(!screen.is_open());
    }

    #[test]
    fn toggle_opens_and_closes() {
        let mut screen = SettingsScreen::default();

        screen.toggle(1.0);
        assert!(screen.is_open());
        assert!(screen.on_back(1.1));
        assert!(!screen.is_open());
    }

    #[test]
    fn back_leaves_category_then_closes() {
        let mut screen = SettingsScreen::default();

        screen.toggle(0.0);
        screen.category = Some(CategoryId::Player);

        assert!(screen.on_back(0.1));
        assert!(screen.is_open());
        assert_eq!(screen.category, None);
        assert_eq!(screen.left_category, Some(CategoryId::Player));

        assert!(screen.on_back(0.2));
        assert!(!screen.is_open());

        assert!(screen.on_back(0.3));
        assert!(!screen.is_open());
    }

    #[test]
    fn back_ignored_when_closed() {
        let mut screen = SettingsScreen::default();

        assert!(!screen.on_back(0.0));
    }
}
