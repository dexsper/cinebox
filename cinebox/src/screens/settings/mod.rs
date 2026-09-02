//! Settings as a right-hand overlay drawer. Fields come from [`catalog`].

mod catalog;
mod controls;
mod speed;

use cinebox_core::i18n::Msg;
use cinebox_core::{ParserKind, PosterSize, QualityBand, UiLanguage};
use egui::Ui;
use egui_async::Bind;
use egui_material_icons::icons::{ICON_KEY, ICON_NETWORK_PING, ICON_SEARCH};

use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::drawer::Overlay;
use crate::widgets::scroll;

use catalog::{CategoryId, Field, MultiSelectId, SelectId, catalog, category};
use controls::{
    Labeled, category_row, clear_cache_row, drawer_title, error_line, multiselect_chip_row,
    nav_header, probe_row, secret_row, select_row, select_row_with, speed_test_row, text_row,
    toggle_row,
};
use speed::SpeedMeter;

pub struct SettingsScreen {
    overlay: Overlay,
    category: Option<CategoryId>,
    torr: Bind<String, String>,
    parser: Bind<String, String>,
    tmdb: Bind<String, String>,
    speed: Bind<(), String>,
    speed_meter: SpeedMeter,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self {
            overlay: Overlay::default(),
            category: None,
            torr: Bind::new(true),
            parser: Bind::new(true),
            tmdb: Bind::new(true),
            speed: Bind::new(true),
            speed_meter: SpeedMeter::new(),
        }
    }
}

impl SettingsScreen {
    pub fn is_open(&self) -> bool {
        self.overlay.is_open()
    }

    pub fn toggle(&mut self, now: f64) {
        self.overlay.toggle(now);
    }

    /// Consume Escape / chrome Back. `true` if the drawer handled it.
    pub fn on_back(&mut self, now: f64) -> bool {
        if !self.overlay.is_blocking(now) {
            return false;
        }

        if !self.overlay.is_open() {
            return true;
        }

        if self.category.take().is_some() {
            return true;
        }

        self.overlay.begin_close(now);
        true
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        let now = ui.input(|i| i.time);
        if !self.overlay.is_blocking(now) {
            self.category = None;
        }

        let mut overlay = std::mem::take(&mut self.overlay);
        overlay.paint(ui, theme, "cinebox-settings", |ui, theme| {
            self.paint_body(ui, svc, theme);
        });
        self.overlay = overlay;
    }

    fn paint_body(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        if let Some(id) = self.category {
            self.paint_category(ui, svc, theme, id);
            return;
        }

        self.paint_list(ui, svc, theme);
    }

    fn paint_list(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme) {
        drawer_title(ui, theme, Msg::SettingsTitle.t());
        ui.add_space(4.0);
        paint_errors(ui, svc, theme);
        ui.add_space(12.0);

        scroll::vertical(ui, "settings-categories", |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for cat in catalog() {
                if category_row(ui, theme, cat) {
                    self.category = Some(cat.id);
                }
            }
        });
    }

    fn paint_category(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        id: CategoryId,
    ) {
        let cat = category(id);
        if nav_header(ui, theme, cat.title.t()) {
            self.category = None;
            return;
        }

        ui.add_space(4.0);
        paint_errors(ui, svc, theme);
        ui.add_space(8.0);

        let mut persist = false;
        scroll::vertical(ui, ("settings-fields", cat.title.en()), |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            for field in cat.fields {
                persist |= self.paint_field(ui, svc, theme, field);
            }
        });

        if persist {
            svc.persist();
        }
    }

    fn paint_field(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        field: &Field,
    ) -> bool {
        match field {
            Field::Toggle {
                label,
                hint,
                get,
                set,
            } => {
                let mut value = get(&svc.settings);
                if !toggle_row(ui, theme, label.t(), hint.map(Msg::t), &mut value) {
                    return false;
                }
                set(&mut svc.settings, value);
                true
            }
            Field::Text {
                label,
                hint,
                placeholder,
                get,
                set,
            } => {
                let mut value = get(&svc.settings);
                if !text_row(ui, theme, label.t(), hint.map(Msg::t), placeholder, &mut value) {
                    return false;
                }
                set(&mut svc.settings, value);
                true
            }
            Field::Secret {
                label,
                hint,
                get,
                set,
            } => {
                let mut value = get(&svc.settings);
                if !secret_row(ui, theme, label.t(), hint.map(Msg::t), &mut value) {
                    return false;
                }
                set(&mut svc.settings, value);
                true
            }
            Field::Select {
                id,
                label,
                hint,
                which,
            } => paint_select(ui, svc, theme, id, label.t(), hint.map(Msg::t), which),
            Field::MultiSelect {
                id,
                label,
                hint,
                which,
            } => paint_multiselect(ui, svc, theme, id, label.t(), hint.map(Msg::t), which),
            Field::ProbeParser => {
                let label = Msg::TestParser.t();
                probe_row(ui, theme, ICON_SEARCH, label, &mut self.parser, || {
                    crate::jobs::ping_parser(svc.settings.clone())
                });
                false
            }
            Field::ProbeTorr => {
                let label = Msg::Ping.t();
                probe_row(ui, theme, ICON_NETWORK_PING, label, &mut self.torr, || {
                    crate::jobs::ping_torrserver(svc.settings.clone())
                });
                false
            }
            Field::ProbeTmdb => {
                let settings = svc.settings.clone();
                let db = svc.db.clone();
                let label = Msg::CheckApiKey.t();
                probe_row(ui, theme, ICON_KEY, label, &mut self.tmdb, || {
                    crate::jobs::ping_tmdb(settings, db)
                });
                false
            }
            Field::SpeedTest => {
                let settings = svc.settings.clone();
                let meter = self.speed_meter.clone();
                let ctx = ui.ctx().clone();
                speed_test_row(ui, theme, &self.speed_meter, &mut self.speed, move || {
                    speed::run(settings, meter, ctx)
                });
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

fn paint_select(
    ui: &mut Ui,
    svc: &mut Services,
    theme: &Theme,
    id: &str,
    label: &str,
    hint: Option<&str>,
    which: &SelectId,
) -> bool {
    match which {
        SelectId::Language => select_row_with(
            ui,
            theme,
            Labeled { id, label, hint },
            &mut svc.settings.general.language,
            UiLanguage::ALL,
            |lang| ui_lang_label(lang).to_owned(),
        ),
        SelectId::ParserKind => select_row(
            ui,
            theme,
            id,
            label,
            hint,
            &mut svc.settings.parser.kind,
            ParserKind::ALL,
        ),
        SelectId::PosterSize => select_row(
            ui,
            theme,
            id,
            label,
            hint,
            &mut svc.settings.tmdb.poster_size,
            PosterSize::ALL,
        ),
    }
}

fn paint_multiselect(
    ui: &mut Ui,
    svc: &mut Services,
    theme: &Theme,
    id: &str,
    label: &str,
    hint: Option<&str>,
    which: &MultiSelectId,
) -> bool {
    match which {
        MultiSelectId::Quality => multiselect_chip_row(
            ui,
            theme,
            Labeled { id, label, hint },
            &mut svc.settings.parser.default_quality,
            QualityBand::ALL,
            |band| band.label().to_owned(),
        ),
    }
}

fn ui_lang_label(lang: UiLanguage) -> &'static str {
    match lang {
        UiLanguage::English => Msg::LangEnglish.t(),
        UiLanguage::Russian => Msg::LangRussian.t(),
    }
}

fn paint_errors(ui: &mut Ui, svc: &Services, theme: &Theme) {
    if let Some(error) = &svc.load_error {
        error_line(ui, theme, Msg::SettingsLoadError.t());
        error_line(ui, theme, error);
    }
    if let Some(error) = &svc.save_error {
        error_line(ui, theme, &format!("{} {error}", Msg::CouldNotSave.t()));
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
