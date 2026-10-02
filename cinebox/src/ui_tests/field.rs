use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::platform::TextPurpose;
use crate::theme::Theme;
use crate::widgets::field;

const STEP: f32 = 0.1;

struct FieldState {
    theme: Theme,
    value: String,
    commits: Vec<String>,
}

fn field_harness() -> Harness<'static, FieldState> {
    Harness::builder()
        .with_size(vec2(400.0, 120.0))
        .with_step_dt(STEP)
        .build_ui_state(
            |ui, state| {
                let FieldState { theme, value, .. } = state;
                let committed = field::committed_edit(ui, theme, field_id(), value, "", TextPurpose::Text);
                if let Some(text) = committed {
                    state.value.clone_from(&text);
                    state.commits.push(text);
                }
            },
            FieldState {
                theme: Theme::dark(),
                value: String::new(),
                commits: Vec::new(),
            },
        )
}

fn field_id() -> egui::Id {
    egui::Id::new("field-under-test")
}

fn steps(harness: &mut Harness<'_, FieldState>, n: usize) {
    for _ in 0..n {
        harness.step();
    }
}

#[test]
fn typing_commits_once_after_idle() {
    let mut harness = field_harness();
    harness.step();

    let edit = harness.get_by_role(Role::TextInput);
    edit.focus();
    edit.type_text("0123456789");
    steps(&mut harness, 5);
    assert!(harness.state().commits.is_empty(), "committed while typing");

    steps(&mut harness, 5);
    assert_eq!(harness.state().commits, vec![String::from("0123456789")]);

    steps(&mut harness, 10);
    assert_eq!(harness.state().commits.len(), 1, "idle must not re-commit");
}

#[test]
fn enter_commits_without_waiting() {
    let mut harness = field_harness();
    harness.step();

    let edit = harness.get_by_role(Role::TextInput);
    edit.focus();
    edit.type_text("abc");
    harness.step();
    harness.key_press(egui::Key::Enter);
    harness.step();

    assert_eq!(harness.state().commits, vec![String::from("abc")]);
}
