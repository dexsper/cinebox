//! The TV form: D-pad focus, the remote's Back, and text fields that do not trap it.

use egui::accesskit::Role;
use egui::{Area, Id, Key, Order, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use std::sync::Arc;

use cinebox_core::UiLanguage;
use rust_i18n::t;

use crate::platform::{self, Form, Host};
use crate::screens::OnboardingScreen;
use crate::services::{Services, db_block_on};
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::{field, focus};

#[derive(Default)]
struct TvState {
    theme: Option<Theme>,
    clicked: Vec<&'static str>,
    popup: bool,
    value: String,
}

fn tv_host() -> Host {
    Host {
        form: Form::Tv,
        keep_awake: None,
    }
}

fn harness(add: fn(&mut egui::Ui, &mut TvState)) -> Harness<'static, TvState> {
    let mut harness = Harness::builder()
        .with_size(vec2(640.0, 360.0))
        .build_ui_state(
            move |ui, state| {
                platform::set(ui.ctx(), tv_host());
                if state.theme.is_none() {
                    crate::fonts::install(ui.ctx());
                    egui_material_icons::initialize(ui.ctx());
                    let theme = Theme::dark();
                    theme.apply(ui.ctx());
                    state.theme = Some(theme);
                    return;
                }

                add(ui, state);
                focus::navigate(ui.ctx());
            },
            TvState::default(),
        );
    harness.run();
    harness
}

/// One frame, through the same hook the app runs before each frame.
fn frame<S>(harness: &mut Harness<'_, S>) {
    let ctx = harness.ctx.clone();
    focus::before_pass(&ctx, harness.input_mut());
    harness.step();
}

/// One remote key press, then a frame for the focus to settle.
fn press<S>(harness: &mut Harness<'_, S>, key: Key) {
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
    }

    // Like the app's repaints: a popup takes the focus a frame after it opens.
    for _ in 0..3 {
        frame(harness);
    }
}

fn focused<S>(harness: &Harness<'_, S>, label: &str) -> bool {
    harness.get_by_label(label).is_focused()
}

fn row_of_buttons(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    ui.horizontal(|ui| {
        for label in ["One", "Two", "Three"] {
            if button::label(ui, &theme, label, Opts::secondary(vec2(80.0, 32.0))) {
                state.clicked.push(label);
            }
        }
    });
}

#[test]
fn first_press_only_places_focus() {
    let mut harness = harness(row_of_buttons);

    press(&mut harness, Key::ArrowRight);
    assert!(
        focused(&harness, "One"),
        "focus should start on the first widget"
    );

    press(&mut harness, Key::ArrowRight);
    assert!(focused(&harness, "Two"));
    assert!(harness.state().clicked.is_empty());
}

#[test]
fn ok_clicks_the_focused_widget() {
    let mut harness = harness(row_of_buttons);

    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::Enter);

    assert_eq!(harness.state().clicked, vec!["Two"]);
}

#[test]
fn starting_focus_prefers_the_screen_body() {
    let mut harness = harness(|ui, state| {
        let Some(theme) = state.theme.clone() else {
            return;
        };

        let _ = button::label(ui, &theme, "Header", Opts::secondary(vec2(80.0, 32.0)));
        let body = ui
            .scope(|ui| {
                let _ = button::label(ui, &theme, "Body", Opts::secondary(vec2(80.0, 32.0)));
            })
            .response
            .rect;
        focus::set_content(ui.ctx(), body);
    });

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Body"));
}

fn field_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    let id = Id::new("tv-field");
    if let Some(text) = field::committed_edit(ui, &theme, id, &state.value, "", false) {
        state.value = text;
    }
    let _ = button::label(ui, &theme, "Below", Opts::secondary(vec2(80.0, 32.0)));
}

#[test]
fn text_field_waits_for_ok_before_typing() {
    let mut harness = harness(field_ui);

    press(&mut harness, Key::ArrowDown);
    assert!(
        !harness.ctx.text_edit_focused(),
        "the D-pad must not land inside the edit"
    );

    press(&mut harness, Key::ArrowDown);
    assert!(
        focused(&harness, "Below"),
        "arrows pass over a field that is not being edited"
    );

    press(&mut harness, Key::ArrowUp);
    press(&mut harness, Key::Enter);
    assert!(harness.ctx.text_edit_focused(), "OK starts typing");

    harness.get_by_role(Role::TextInput).type_text("tmdb");
    frame(&mut harness);
    press(&mut harness, Key::BrowserBack);

    assert!(!harness.ctx.text_edit_focused(), "Back stops typing");
    assert_eq!(harness.state().value, "tmdb");

    press(&mut harness, Key::ArrowDown);
    assert!(
        focused(&harness, "Below"),
        "the D-pad continues from the field"
    );
}

fn popup_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    ui.horizontal(|ui| {
        if button::label(ui, &theme, "Open", Opts::secondary(vec2(80.0, 32.0))) {
            state.popup = true;
        }
        let _ = button::label(ui, &theme, "Behind", Opts::secondary(vec2(80.0, 32.0)));
    });

    if !state.popup {
        return;
    }

    Area::new(Id::new("tv-popup"))
        .order(Order::Foreground)
        .fixed_pos(egui::pos2(40.0, 120.0))
        .show(ui.ctx(), |ui| {
            focus::trap(ui);
            ui.horizontal(|ui| {
                let _ = button::label(ui, &theme, "Inside", Opts::secondary(vec2(80.0, 32.0)));
                let _ = button::label(ui, &theme, "Also", Opts::secondary(vec2(80.0, 32.0)));
            });
        });

    if ui.input(|i| i.key_pressed(Key::Escape)) {
        state.popup = false;
    }
}

#[test]
fn popup_keeps_the_dpad_and_gives_it_back() {
    let mut harness = harness(popup_ui);

    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::Enter);
    assert!(harness.state().popup);
    assert!(focused(&harness, "Inside"), "focus moves into the popup");

    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowUp);
    assert!(
        focused(&harness, "Also"),
        "the D-pad cannot reach widgets behind the popup"
    );

    press(&mut harness, Key::BrowserBack);
    frame(&mut harness);
    assert!(!harness.state().popup);
    assert!(
        focused(&harness, "Open"),
        "focus returns to what opened the popup"
    );
}

struct WizardTv {
    wizard: OnboardingScreen,
    svc: Services,
    theme: Theme,
    fonts: bool,
}

fn wizard_harness(language: UiLanguage) -> Harness<'static, WizardTv> {
    let Ok(store) = db_block_on(cinebox_core::Store::memory()) else {
        panic!("in-memory store");
    };

    let mut wizard = OnboardingScreen::default();
    wizard.open_offline();
    let mut svc = Services::test_with_db(Arc::new(store));
    svc.settings.general.language = language;

    let state = WizardTv {
        wizard,
        svc,
        theme: Theme::dark(),
        fonts: false,
    };

    let mut harness = Harness::builder()
        .with_size(vec2(1000.0, 800.0))
        .build_ui_state(draw_wizard_over_screen, state);

    for _ in 0..3 {
        frame(&mut harness);
    }

    harness
}

/// The wizard over a screen full of focusable rows, like Home behind it.
fn draw_wizard_over_screen(ui: &mut egui::Ui, state: &mut WizardTv) {
    platform::set(ui.ctx(), tv_host());
    if !state.fonts {
        crate::fonts::install(ui.ctx());
        egui_material_icons::initialize(ui.ctx());
        state.theme.apply(ui.ctx());
        state.fonts = true;
        return;
    }

    ui.vertical_centered(|ui| {
        for row in 0..24 {
            let label = format!("Behind {row}");
            let opts = Opts::secondary(vec2(400.0, 20.0));
            let _ = button::label(ui, &state.theme, &label, opts);
        }
    });

    let ctx = ui.ctx().clone();
    state.wizard.ui(&ctx, &mut state.svc, &state.theme);
    focus::navigate(&ctx);
}

#[test]
fn wizard_starts_on_the_current_language() {
    let harness = wizard_harness(UiLanguage::Russian);

    assert!(focused(&harness, "Русский"));
}

#[test]
fn wizard_dpad_walks_the_card_not_the_screen_behind() {
    let mut harness = wizard_harness(UiLanguage::Russian);

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Українська"));

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, &t!("wizard.next")));

    press(&mut harness, Key::ArrowUp);
    press(&mut harness, Key::ArrowUp);
    press(&mut harness, Key::ArrowUp);
    assert!(focused(&harness, "English"));
}
