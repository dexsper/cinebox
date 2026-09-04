use cinebox_core::HomeRowId;
use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use rust_i18n::t;

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

                let clicked =
                    crate::screens::home::shelf_heading(ui, HomeRowId::NowPlaying, &state.theme);
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
