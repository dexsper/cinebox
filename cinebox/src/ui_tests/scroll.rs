use egui::{Event, Modifiers, MouseWheelUnit, TouchPhase, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::widgets::scroll;

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

fn settle_label<S>(harness: &mut Harness<'_, S>, label: &str) -> egui::Rect {
    let mut last = f32::MAX;
    let mut stable = 0;
    for _ in 0..120 {
        harness.run();
        let y = harness.get_by_label(label).rect().min.y;
        if (y - last).abs() < 0.4 {
            stable += 1;
            if stable >= 5 {
                break;
            }
        } else {
            stable = 0;
        }
        last = y;
    }
    harness.get_by_label(label).rect()
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
fn shift_wheel_pans_lower_shelf_after_page_scroll() {
    // Home shows ~2 shelves; the rest need a vertical scroll first.
    // Shift+wheel must pan the hovered shelf, not the first two that were on screen.
    let mut harness = Harness::builder()
        .with_size(vec2(640.0, 220.0))
        .build_ui(|ui| {
            scroll::vertical(ui, "page", |ui| {
                for prefix in ["A", "B", "C", "D", "E"] {
                    scroll::horizontal(ui, format!("row-{prefix}"), |ui| {
                        wide_row(ui, prefix);
                    });
                    ui.add_space(16.0);
                }
            });
        });
    harness.run();

    let viewport_h = 220.0;
    let d_y0 = harness.get_by_label("D0").rect().min.y;
    assert!(
        d_y0 > viewport_h,
        "D must start below the viewport so the test actually scrolls, y={d_y0}"
    );

    let a_rect = harness.get_by_label("A0").rect();
    harness.hover_at(a_rect.center());
    for _ in 0..6 {
        harness.event(Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: vec2(0.0, -1.0),
            phase: TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        harness.run_steps(4);
    }
    let d_rect = settle_label(&mut harness, "D0");
    assert!(
        d_rect.center().y > 0.0 && d_rect.center().y < viewport_h,
        "D should be visible after page scroll, rect={d_rect:?}"
    );

    let a_before = node_x(&harness, "A0");
    let b_before = node_x(&harness, "B0");
    let d_before = node_x(&harness, "D0");
    harness.hover_at(d_rect.center());
    harness.run();
    shift_wheel(&harness, -1.0);
    harness.run_steps(12);

    let a_after = node_x(&harness, "A0");
    let b_after = node_x(&harness, "B0");
    let d_after = node_x(&harness, "D0");
    assert!(
        (a_after - a_before).abs() < 3.0,
        "shelf A must stay put after scrolling away, {a_before} -> {a_after}"
    );
    assert!(
        (b_after - b_before).abs() < 3.0,
        "shelf B must stay put after scrolling away, {b_before} -> {b_after}"
    );
    assert!(
        (d_after - d_before).abs() > 8.0,
        "hovered lower shelf D should pan, {d_before} -> {d_after}"
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
