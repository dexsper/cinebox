use std::sync::Arc;

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use rust_i18n::t;

use crate::screens::OnboardingScreen;
use crate::services::{Services, db_block_on};
use crate::theme::Theme;

struct WizardState {
    wizard: OnboardingScreen,
    svc: Services,
    theme: Theme,
    fonts: bool,
}

fn wizard_harness() -> Harness<'static, WizardState> {
    let Ok(store) = db_block_on(cinebox_core::Store::memory()) else {
        panic!("in-memory store");
    };

    let mut wizard = OnboardingScreen::default();
    wizard.open_offline();

    let state = WizardState {
        wizard,
        svc: Services::test_with_db(Arc::new(store)),
        theme: Theme::dark(),
        fonts: false,
    };

    Harness::builder()
        .with_size(vec2(1000.0, 800.0))
        .build_ui_state(draw_wizard, state)
}

fn draw_wizard(ui: &mut egui::Ui, state: &mut WizardState) {
    if !state.fonts {
        crate::fonts::install(ui.ctx());
        egui_material_icons::initialize(ui.ctx());
        state.theme.apply(ui.ctx());
        state.fonts = true;
        return;
    }

    let ctx = ui.ctx().clone();
    state.wizard.ui(&ctx, &mut state.svc, &state.theme);
}

fn click(harness: &mut Harness<'_, WizardState>, label: &str) {
    for node in harness.query_all_by_role(Role::Button) {
        eprintln!("BUTTON {:?}", node);
    }

    eprintln!("---- click {label}");
    harness.get_by_role_and_label(Role::Button, label).click();
    harness.run();
}

#[test]
fn walking_through_every_step_finishes_onboarding() {
    let mut harness = wizard_harness();
    harness.run();

    click(&mut harness, &t!("wizard.next"));
    click(&mut harness, &t!("wizard.skip"));
    click(&mut harness, &t!("wizard.skip"));
    click(&mut harness, &t!("wizard.next"));
    click(&mut harness, &t!("wizard.start"));

    assert!(!harness.state().wizard.is_open());
    assert!(harness.state().svc.settings.general.onboarded);
}

#[test]
fn set_up_later_closes_from_any_step() {
    let mut harness = wizard_harness();
    harness.run();

    click(&mut harness, &t!("wizard.next"));
    click(&mut harness, &t!("wizard.later"));

    assert!(!harness.state().wizard.is_open());
    assert!(harness.state().svc.settings.general.onboarded);
}
