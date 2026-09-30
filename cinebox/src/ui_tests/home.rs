use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use rust_i18n::t;

use crate::nav::{NavAction, SettingsPage};
use crate::screens::gate::{self, TmdbKeyProblem};
use crate::theme::Theme;

struct HeadingState {
    theme: Theme,
    fonts: bool,
    clicked: bool,
}

#[test]
fn shelf_heading_click_is_a_button() {
    let mut harness = Harness::builder()
        .with_size(vec2(400.0, 80.0))
        .build_ui_state(
            |ui, state| {
                if !state.fonts {
                    crate::fonts::install(ui.ctx());
                    egui_material_icons::initialize(ui.ctx());
                    state.theme.apply(ui.ctx());
                    state.fonts = true;
                    return;
                }

                let title = t!("home.now_playing");
                let clicked = crate::screens::shelf::shelf_heading(ui, &title, &state.theme);
                if clicked {
                    state.clicked = true;
                }
            },
            HeadingState {
                theme: Theme::dark(),
                fonts: false,
                clicked: false,
            },
        );
    harness.run();

    harness
        .get_by_role_and_label(Role::Button, t!("home.now_playing").as_ref())
        .click();

    harness.run();
    assert!(harness.state().clicked);
}

struct GateState {
    theme: Theme,
    fonts: bool,
    action: Option<NavAction>,
}

#[test]
fn missing_tmdb_key_gate_opens_tmdb_settings() {
    let mut harness = Harness::builder()
        .with_size(vec2(800.0, 600.0))
        .build_ui_state(
            |ui, state| {
                if !state.fonts {
                    crate::fonts::install(ui.ctx());
                    egui_material_icons::initialize(ui.ctx());
                    state.theme.apply(ui.ctx());
                    state.fonts = true;
                    return;
                }

                let problem = TmdbKeyProblem::Missing;
                let action = gate::tmdb_key(ui, &state.theme, problem);
                if action.is_some() {
                    state.action = action;
                }
            },
            GateState {
                theme: Theme::dark(),
                fonts: false,
                action: None,
            },
        );
    harness.run();

    let label = t!("gate.tmdb_open_settings");
    harness.get_by_role_and_label(Role::Button, &label).click();
    harness.run();

    let expected = NavAction::OpenSettingsAt(SettingsPage::Tmdb);
    assert_eq!(harness.state().action, Some(expected));
}
