use egui::accesskit::Role;
use egui::{Sense, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use rust_i18n::t;

use crate::theme::Theme;
use crate::widgets;

const TOOLBAR_H: f32 = 40.0;
const PANE_W: f32 = 640.0;
const PANE_H: f32 = 480.0;

struct PaneState {
    theme: Theme,
    fonts: bool,
    toolbar_bottom: f32,
    retry: bool,
    show_error: bool,
}

fn pane_harness(show_error: bool) -> Harness<'static, PaneState> {
    let mut harness = Harness::builder()
        .with_size(vec2(PANE_W, PANE_H))
        .build_ui_state(
            |ui, state| {
                if !state.fonts {
                    crate::fonts::install(ui.ctx());
                    egui_material_icons::initialize(ui.ctx());
                    state.theme.apply(ui.ctx());
                    state.fonts = true;
                    return;
                }

                let theme = &state.theme;
                let bar_size = vec2(ui.available_width(), TOOLBAR_H);
                let (bar, _) = ui.allocate_exact_size(bar_size, Sense::hover());
                state.toolbar_bottom = bar.bottom();

                if state.show_error {
                    if widgets::page_error(ui, theme, t!("torrents.need_parser").as_ref()) {
                        state.retry = true;
                    }
                    return;
                }

                widgets::page_spinner(ui, theme);
            },
            PaneState {
                theme: Theme::dark(),
                fonts: false,
                toolbar_bottom: 0.0,
                retry: false,
                show_error,
            },
        );
    harness.run_steps(4);
    harness
}

fn remaining_mid_y(harness: &Harness<'_, PaneState>) -> f32 {
    let top = harness.state().toolbar_bottom;
    top + (PANE_H - top) * 0.5
}

#[test]
fn spinner_centers_below_toolbar() {
    let harness = pane_harness(false);
    let spinner = harness.get_by_role(Role::ProgressIndicator).rect();
    let mid = remaining_mid_y(&harness);

    assert!(
        (spinner.center().y - mid).abs() < 24.0,
        "spinner y {} vs pane mid {}",
        spinner.center().y,
        mid
    );
}

#[test]
fn parser_error_centers_below_toolbar() {
    let mut harness = pane_harness(true);
    let retry = harness
        .get_by_role_and_label(Role::Button, t!("common.retry").as_ref())
        .rect();
    let copy = harness.get_by_label(t!("torrents.need_parser").as_ref()).rect();
    let group_mid = (copy.center().y + retry.center().y) * 0.5;
    let mid = remaining_mid_y(&harness);

    assert!(
        (group_mid - mid).abs() < 36.0,
        "error group y {group_mid} vs pane mid {mid}"
    );

    harness
        .get_by_role_and_label(Role::Button, t!("common.retry").as_ref())
        .click();
    harness.run();
    assert!(harness.state().retry);
}
