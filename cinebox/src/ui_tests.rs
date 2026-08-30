//! UI tests via `egui_kittest`. Interaction only — no GPU snapshots.

use egui::accesskit::Role;
use egui::{Event, Modifiers, MouseWheelUnit, TouchPhase, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::nav::{NavAction, Screen};
use crate::theme::Theme;
use crate::toasts::Toasts;
use crate::widgets::{chrome, scroll};

struct HeaderState {
    screen: Screen,
    theme: Theme,
    action: Option<NavAction>,
    fonts: bool,
}

fn header_harness(screen: Screen) -> Harness<'static, HeaderState> {
    let mut harness = Harness::builder()
        .with_size(vec2(800.0, 72.0))
        .build_ui_state(
            |ui, state| {
                if !state.fonts {
                    egui_material_icons::initialize(ui.ctx());
                    state.theme.apply(ui.ctx());
                    state.fonts = true;
                    return;
                }
                if let Some(nav) = chrome::header(ui, state.screen, &state.theme) {
                    state.action = Some(nav);
                }
            },
            HeaderState {
                screen,
                theme: Theme::dark(),
                action: None,
                fonts: false,
            },
        );
    harness.run();
    harness
}

fn shift_wheel<S>(harness: &Harness<'_, S>, dy: f32) {
    harness.event_modifiers(
        Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: vec2(0.0, dy),
            phase: TouchPhase::Move,
            modifiers: Modifiers::SHIFT,
        },
        Modifiers::SHIFT,
    );
}

fn node_x<S>(harness: &Harness<'_, S>, label: &str) -> f32 {
    harness.get_by_label(label).rect().min.x
}

fn wide_row(ui: &mut egui::Ui, prefix: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for i in 0..16 {
            ui.add_sized([140.0, 72.0], egui::Button::new(format!("{prefix}{i}")));
        }
    });
}

#[test]
fn home_header_settings_click_opens_settings() {
    let mut harness = header_harness(Screen::Home);
    harness.get_by_role(Role::Button).click();
    harness.run();
    assert_eq!(harness.state().action, Some(NavAction::OpenSettings));
}

#[test]
fn media_header_back_click_goes_back() {
    let mut harness = header_harness(Screen::Media {
        kind: cinebox_core::MediaKind::Movie,
        id: cinebox_core::TmdbId::new(1),
    });
    harness.get_by_role(Role::Button).click();
    harness.run();
    assert_eq!(harness.state().action, Some(NavAction::GoBack));
}

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

#[test]
fn shift_wheel_pans_hovered_shelf_not_the_first() {
    let mut harness = Harness::builder()
        .with_size(vec2(640.0, 520.0))
        .build_ui(|ui| {
            scroll::vertical(ui, "page", |ui| {
                scroll::horizontal(ui, "row-a", |ui| wide_row(ui, "A"));
                ui.add_space(16.0);
                scroll::horizontal(ui, "row-b", |ui| wide_row(ui, "B"));
            });
        });
    harness.run();

    let a_before = node_x(&harness, "A0");
    let b_before = node_x(&harness, "B0");
    let b_rect = harness.get_by_label("B0").rect();
    harness.hover_at(b_rect.center());
    shift_wheel(&harness, -1.0);
    harness.run_steps(12);

    let a_after = node_x(&harness, "A0");
    let b_after = node_x(&harness, "B0");
    assert!(
        (a_after - a_before).abs() < 3.0,
        "first shelf must stay put, {a_before} -> {a_after}"
    );
    assert!(
        (b_after - b_before).abs() > 8.0,
        "hovered shelf should pan, {b_before} -> {b_after}"
    );
}

#[test]
fn vertical_wheel_over_shelf_scrolls_the_page() {
    let mut harness = Harness::builder()
        .with_size(vec2(640.0, 140.0))
        .build_ui(|ui| {
            scroll::vertical(ui, "page", |ui| {
                scroll::horizontal(ui, "row-a", |ui| wide_row(ui, "A"));
                ui.add_space(24.0);
                scroll::horizontal(ui, "row-b", |ui| wide_row(ui, "B"));
                ui.add_space(24.0);
                ui.label("Bottom");
            });
        });
    harness.run();

    let bottom_before = harness.get_by_label("Bottom").rect().min.y;
    let a_rect = harness.get_by_label("A0").rect();
    harness.hover_at(a_rect.center());
    harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, -1.0),
        phase: TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    harness.run_steps(12);

    let bottom_after = harness.get_by_label("Bottom").rect().min.y;
    assert!(
        bottom_after + 8.0 < bottom_before,
        "page should scroll, bottom {bottom_before} -> {bottom_after}"
    );
}
