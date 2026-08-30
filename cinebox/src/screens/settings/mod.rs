//! Settings as a right-hand overlay drawer. Fields come from [`catalog`].

mod catalog;
mod controls;
mod speed;

use cinebox_core::i18n::Msg;
use cinebox_core::{DefaultQuality, ParserKind, PosterSize, UiLanguage, VideoScale};
use egui::{Area, Frame, Id, Margin, Order, Rect, Sense, Ui, UiBuilder, pos2};
use egui_async::Bind;
use egui_material_icons::icons::{ICON_KEY, ICON_NETWORK_PING, ICON_SEARCH};

use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{intro, scroll};

use catalog::{CategoryId, Field, SelectId, catalog, category};
use controls::{
    category_row, clear_cache_row, data_language_row, drawer_title, error_line, nav_header,
    probe_row, secret_row, select_row, speed_test_row, text_row, toggle_row,
};
use speed::SpeedMeter;

const DRAWER_FRAC: f32 = 0.4;
const DRAWER_MIN: f32 = 340.0;
const DRAWER_MAX: f32 = 520.0;

pub struct SettingsScreen {
    want_open: bool,
    anim_at: Option<f64>,
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
            want_open: false,
            anim_at: None,
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
        self.want_open
    }

    pub fn animating(&self, now: f64) -> bool {
        intro::running(self.anim_at, now) && self.visual_t(now) > 0.0
    }

    pub fn toggle(&mut self, now: f64) {
        if self.want_open {
            self.begin_close(now);
            return;
        }

        self.begin_open(now);
    }

    /// Consume Escape / chrome Back. `true` if the drawer handled it.
    pub fn on_back(&mut self, now: f64) -> bool {
        if !self.is_blocking(now) {
            return false;
        }

        if !self.want_open {
            return true;
        }

        if self.category.take().is_some() {
            return true;
        }

        self.begin_close(now);
        true
    }

    pub fn ui(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        let now = ui.input(|i| i.time);
        self.finish_close(now);

        let t = self.visual_t(now);
        if t <= 0.001 {
            return;
        }

        if self.animating(now) {
            ui.ctx().request_repaint();
        }

        let full = ui.ctx().content_rect();
        let body = Rect::from_min_max(
            pos2(full.left(), full.top() + theme.title_bar_h),
            full.right_bottom(),
        );
        let width = (body.width() * DRAWER_FRAC).clamp(DRAWER_MIN, DRAWER_MAX);
        let shown = width * t;
        let drawer_left = body.right() - shown;
        let dim_rect = Rect::from_min_max(body.left_top(), pos2(drawer_left, body.bottom()));
        let drawer_rect = Rect::from_min_max(pos2(drawer_left, body.top()), body.right_bottom());

        Area::new(Id::new("cinebox-settings"))
            .order(Order::Foreground)
            .fixed_pos(body.min)
            .constrain(false)
            .show(ui.ctx(), |ui| {
                ui.set_min_size(body.size());
                ui.set_clip_rect(body);

                if dim_rect.width() > 1.0 {
                    ui.painter().rect_filled(dim_rect, 0.0, theme.overlay_at(t));
                    let dim = ui.interact(dim_rect, Id::new("cinebox-settings-dim"), Sense::click());
                    if dim.clicked() {
                        self.begin_close(now);
                    }
                }

                if drawer_rect.width() < 8.0 {
                    return;
                }

                ui.scope_builder(UiBuilder::new().max_rect(drawer_rect), |ui| {
                    ui.set_min_size(drawer_rect.size());
                    ui.set_max_size(drawer_rect.size());
                    Frame::new()
                        .fill(theme.panel_elevated)
                        .inner_margin(Margin::symmetric(20, 16))
                        .show(ui, |ui| {
                            self.paint_body(ui, svc, theme);
                        });
                });
            });
    }

    fn begin_open(&mut self, now: f64) {
        let current = self.visual_t(now);
        self.want_open = true;
        self.anim_at = Some(intro::started_at(now, current));
    }

    fn begin_close(&mut self, now: f64) {
        let current = self.visual_t(now);
        self.want_open = false;
        self.anim_at = Some(intro::started_at(now, 1.0 - current));
    }

    fn finish_close(&mut self, now: f64) {
        if self.want_open {
            return;
        }

        if intro::running(self.anim_at, now) {
            return;
        }

        self.anim_at = None;
        self.category = None;
    }

    fn is_blocking(&self, now: f64) -> bool {
        self.want_open || self.visual_t(now) > 0.02
    }

    fn visual_t(&self, now: f64) -> f32 {
        let t = intro::t(self.anim_at, now);
        if self.want_open {
            return t;
        }

        1.0 - t
    }

    fn paint_body(&mut self, ui: &mut Ui, svc: &mut Services, theme: &Theme) {
        if let Some(id) = self.category {
            self.paint_category(ui, svc, theme, id);
            return;
        }

        self.paint_list(ui, svc, theme);
    }

    fn paint_list(&mut self, ui: &mut Ui, svc: &Services, theme: &Theme) {
        drawer_title(ui, theme, Msg::SettingsTitle.en());
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
        if nav_header(ui, theme, cat.title) {
            self.category = None;
            return;
        }

        ui.add_space(4.0);
        paint_errors(ui, svc, theme);
        ui.add_space(8.0);

        let mut persist = false;
        scroll::vertical(ui, ("settings-fields", cat.title), |ui| {
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
                if !toggle_row(ui, theme, label, *hint, &mut value) {
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
                if !text_row(ui, theme, label, *hint, placeholder, &mut value) {
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
                if !secret_row(ui, theme, label, *hint, &mut value) {
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
            } => paint_select(ui, svc, theme, id, label, *hint, which),
            Field::DataLanguage => {
                data_language_row(ui, theme, &mut svc.settings.tmdb.data_language)
            }
            Field::ProbeParser => {
                probe_row(ui, theme, ICON_SEARCH, "Test parser", &mut self.parser, || {
                    crate::jobs::ping_parser(svc.settings.clone())
                });
                false
            }
            Field::ProbeTorr => {
                probe_row(ui, theme, ICON_NETWORK_PING, "Ping", &mut self.torr, || {
                    crate::jobs::ping_torrserver(svc.settings.clone())
                });
                false
            }
            Field::ProbeTmdb => {
                let settings = svc.settings.clone();
                let db = svc.db.clone();
                probe_row(ui, theme, ICON_KEY, "Check API key", &mut self.tmdb, || {
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
        SelectId::Language => select_row(
            ui,
            theme,
            id,
            label,
            hint,
            &mut svc.settings.interface.language,
            UiLanguage::ALL,
        ),
        SelectId::Scale => select_row(
            ui,
            theme,
            id,
            label,
            hint,
            &mut svc.settings.player.scale,
            VideoScale::ALL,
        ),
        SelectId::Quality => select_row(
            ui,
            theme,
            id,
            label,
            hint,
            &mut svc.settings.player.default_quality,
            DefaultQuality::ALL,
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

fn paint_errors(ui: &mut Ui, svc: &Services, theme: &Theme) {
    if let Some(error) = &svc.load_error {
        error_line(ui, theme, Msg::SettingsLoadError.en());
        error_line(ui, theme, error);
    }
    if let Some(error) = &svc.save_error {
        error_line(ui, theme, &format!("Could not save: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_closed() {
        let screen = SettingsScreen::default();

        assert!(!screen.is_open());
        assert!(!screen.is_blocking(0.0));
        assert!((screen.visual_t(0.0) - 0.0).abs() < f32::EPSILON);
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

        screen.begin_open(0.0);
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
