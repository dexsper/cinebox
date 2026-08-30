use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::theme::Theme;
use crate::toasts::Toasts;

#[test]
fn error_toast_shows_and_dismisses_on_click() {
    struct State {
        toasts: Toasts,
        theme: Theme,
        seeded: bool,
    }

    let mut harness = Harness::builder()
        .with_size(vec2(480.0, 320.0))
        .build_ui_state(
            |ui, state| {
                if !state.seeded {
                    let now = ui.input(|i| i.time);
                    state.toasts.info("hello", now);
                    state.toasts.success("ok", now);
                    state.toasts.error("Network failed", now);
                    state.seeded = true;
                }
                state.toasts.show(ui.ctx(), &state.theme);
            },
            State {
                toasts: Toasts::default(),
                theme: Theme::dark(),
                seeded: false,
            },
        );

    harness.get_by_label("Network failed").click();
    harness.run();
    assert!(harness.query_by_label("Network failed").is_none());
}
