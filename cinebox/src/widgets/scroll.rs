//! Overlay scrollbars, click-drag, Shift+wheel, and inertial coasting.
//!
//! A wheel notch becomes velocity; friction is exponential. Drag stays with
//! egui so nested buttons still receive clicks.

use egui::containers::scroll_area::{DragScroll, ScrollSource};
use egui::{
    AsIdSalt, Event, Id, Modifiers, MouseWheelUnit, Rect, ScrollArea, Ui, Vec2, Vec2b, vec2,
};

const FRICTION: f32 = 4.2;
const MIN_SPEED: f32 = 22.0;
const WHEEL_GAIN: f32 = 10.0;
const PIXEL_GAIN: f32 = 6.0;
const MAX_SPEED: f32 = 4200.0;
const WHEEL_TAKEN: &str = "cinebox-wheel-taken";

#[derive(Clone, Copy, Debug)]
struct Coast {
    vel: Vec2,
    offset: Vec2,
    rect: Rect,
    dragging: bool,
}

impl Default for Coast {
    fn default() -> Self {
        Self {
            vel: Vec2::ZERO,
            offset: Vec2::ZERO,
            rect: Rect::NOTHING,
            dragging: false,
        }
    }
}

impl Coast {
    fn impulse(&mut self, dvel: Vec2) {
        self.vel.x = (self.vel.x + dvel.x).clamp(-MAX_SPEED, MAX_SPEED);
        self.vel.y = (self.vel.y + dvel.y).clamp(-MAX_SPEED, MAX_SPEED);
    }

    fn stop(&mut self) {
        self.vel = Vec2::ZERO;
    }

    fn moving(&self) -> bool {
        self.vel.x.abs().max(self.vel.y.abs()) >= MIN_SPEED
    }

    fn step(&mut self, dt: f32) -> Vec2 {
        let dt = dt.clamp(0.001, 0.05);
        let moved = self.vel * dt;
        self.vel *= (-FRICTION * dt).exp();
        if self.vel.x.abs() < MIN_SPEED {
            self.vel.x = 0.0;
        }
        if self.vel.y.abs() < MIN_SPEED {
            self.vel.y = 0.0;
        }
        moved
    }
}

fn source() -> ScrollSource {
    ScrollSource {
        scroll_bar: true,
        drag: DragScroll::Always,
        mouse_wheel: false,
    }
}

/// Vertical page scroll with overlay bar, drag, and inertia.
pub fn vertical(ui: &mut Ui, id: impl AsIdSalt, add: impl FnOnce(&mut Ui)) {
    show(ui, id, Vec2b::new(false, true), Vec2b::FALSE, None, add);
}

/// Vertical list with a height cap (modals, side panes).
pub fn vertical_max(ui: &mut Ui, id: impl AsIdSalt, max_height: f32, add: impl FnOnce(&mut Ui)) {
    show(
        ui,
        id,
        Vec2b::new(false, true),
        Vec2b::FALSE,
        Some(max_height),
        add,
    );
}

/// Horizontal shelf: height follows content.
pub fn horizontal(ui: &mut Ui, id: impl AsIdSalt, add: impl FnOnce(&mut Ui)) {
    show(ui, id, Vec2b::new(true, false), Vec2b::new(false, true), None, add);
}

fn show(
    ui: &mut Ui,
    salt: impl AsIdSalt,
    enabled: Vec2b,
    auto_shrink: Vec2b,
    max_height: Option<f32>,
    add: impl FnOnce(&mut Ui),
) {
    let coast_id = ui.id().with(("coast", &salt));
    let mut coast: Coast = ui.ctx().data(|d| d.get_temp(coast_id)).unwrap_or_default();

    let pointer_down = ui.input(|i| i.pointer.primary_down());
    if coast.dragging || (pointer_down && pointer_over(ui, coast.rect)) {
        coast.stop();
    }

    let coasting = coast.moving();
    if coasting && !pointer_down {
        let dt = ui.input(|i| i.stable_dt);
        let delta = coast.step(dt);
        coast.offset += delta;
    }

    let mut area = ScrollArea::new(enabled)
        .id_salt(salt)
        .auto_shrink(auto_shrink)
        .scroll_source(source());
    if let Some(height) = max_height {
        area = area.max_height(height);
    }
    if coasting {
        if enabled[0] {
            area = area.horizontal_scroll_offset(coast.offset.x);
        }
        if enabled[1] {
            area = area.vertical_scroll_offset(coast.offset.y);
        }
    }

    let output = area.show(ui, add);
    let hit = hit_rect(output.inner_rect, output.content_size, enabled);
    let hovered = pointer_over(ui, hit);
    let dragging = is_scroll_dragging(ui, output.id);

    if dragging {
        coast.stop();
    } else if !pointer_down && hovered {
        apply_wheel(ui, enabled, &mut coast);
    }

    coast.dragging = dragging;
    coast.rect = hit;
    coast.offset = output.state.offset;

    if coast.moving() {
        ui.ctx().request_repaint();
    }
    ui.ctx().data_mut(|d| d.insert_temp(coast_id, coast));
}

fn pointer_over(ui: &Ui, rect: Rect) -> bool {
    rect.is_positive() && ui.rect_contains_pointer(rect)
}

/// Viewport on scroll axes; content size on the rest so a shelf cannot cover rows below it.
fn hit_rect(inner: Rect, content: Vec2, enabled: Vec2b) -> Rect {
    let mut size = inner.size();
    if !enabled[0] {
        size.x = content.x.min(size.x);
    }
    if !enabled[1] {
        size.y = content.y.min(size.y);
    }
    Rect::from_min_size(inner.min, size)
}

fn is_scroll_dragging(ui: &Ui, scroll_id: Id) -> bool {
    ui.ctx().is_being_dragged(scroll_id.with("area"))
        || ui.ctx().is_being_dragged(scroll_id.with(0_usize))
        || ui.ctx().is_being_dragged(scroll_id.with(1_usize))
}

fn apply_wheel(ui: &Ui, enabled: Vec2b, coast: &mut Coast) {
    let impulses: Vec<Vec2> = ui.input(|i| {
        let shift = i.modifiers.shift;
        i.events
            .iter()
            .filter_map(|event| {
                let Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = *event
                else {
                    return None;
                };
                let mut mods = modifiers;
                mods.shift |= shift;
                wheel_impulse(enabled, unit, delta, mods)
            })
            .collect()
    });
    for dvel in impulses {
        if mark_wheel_taken(ui) {
            continue;
        }
        coast.impulse(dvel);
    }
}

/// Velocity in scroll-offset space (positive = pan down / right).
fn wheel_impulse(
    enabled: impl Into<Vec2b>,
    unit: MouseWheelUnit,
    delta: Vec2,
    modifiers: Modifiers,
) -> Option<Vec2> {
    let enabled = enabled.into();
    let (px, gain) = match unit {
        MouseWheelUnit::Line => (vec2(-delta.x * 60.0, -delta.y * 60.0), WHEEL_GAIN),
        MouseWheelUnit::Point => (vec2(-delta.x, -delta.y), PIXEL_GAIN),
        MouseWheelUnit::Page => (vec2(-delta.x * 800.0, -delta.y * 800.0), PIXEL_GAIN),
    };
    let shift = modifiers.shift;
    let mut dvel = Vec2::ZERO;
    let mut take = false;

    if enabled[0] {
        let dx = if shift { px.x + px.y } else { px.x };
        if dx.abs() > 0.5 {
            dvel.x = dx * gain;
            take = true;
        }
    }
    if enabled[1] && !shift && px.y.abs() > 0.5 {
        dvel.y = px.y * gain;
        take = true;
    }

    take.then_some(dvel)
}

fn mark_wheel_taken(ui: &Ui) -> bool {
    let key = Id::new(WHEEL_TAKEN);
    let pass = ui.ctx().cumulative_pass_nr();
    ui.ctx().data_mut(|d| {
        if d.get_temp::<u64>(key) == Some(pass) {
            true
        } else {
            d.insert_temp(key, pass);
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn scroll_ids_are_stable() {
        let a = Id::new("cinebox-scroll-page");
        let b = Id::new("cinebox-scroll-page");
        assert_eq!(a, b);
    }

    #[test]
    fn shift_converts_vertical_wheel_lines_to_horizontal() {
        let dvel = wheel_impulse(
            [true, false],
            MouseWheelUnit::Line,
            vec2(0.0, 1.0),
            Modifiers::SHIFT,
        )
        .expect("shift+wheel on a row");
        assert!(dvel.x.abs() > 0.5);
        assert_eq!(dvel.y, 0.0);
        assert!(
            wheel_impulse(
                [true, false],
                MouseWheelUnit::Line,
                vec2(0.0, 1.0),
                Modifiers::NONE,
            )
            .is_none()
        );
    }

    #[test]
    fn shift_disables_vertical_wheel_on_page() {
        assert!(
            wheel_impulse(
                [false, true],
                MouseWheelUnit::Line,
                vec2(0.0, 1.0),
                Modifiers::SHIFT,
            )
            .is_none()
        );
        assert!(
            wheel_impulse(
                [false, true],
                MouseWheelUnit::Line,
                vec2(0.0, 1.0),
                Modifiers::NONE,
            )
            .is_some()
        );
    }

    #[test]
    fn horizontal_hit_rect_does_not_cover_rows_below() {
        let inner = Rect::from_min_size(pos2(0.0, 80.0), vec2(800.0, 720.0));
        let hit = hit_rect(inner, vec2(4000.0, 280.0), Vec2b::new(true, false));
        assert_eq!(hit.height(), 280.0);
        assert_eq!(hit.width(), 800.0);
        assert!(
            !hit.contains(pos2(40.0, 400.0)),
            "pointer on a lower row must miss the first shelf"
        );
    }

    #[test]
    fn inertia_decays_and_stops() {
        let mut coast = Coast::default();
        coast.impulse(vec2(0.0, 800.0));
        assert!(coast.moving());
        let mut t = 0.0;
        let mut moved = false;
        for _ in 0..180 {
            t += 1.0 / 60.0;
            if coast.step(1.0 / 60.0).y.abs() > 0.15 {
                moved = true;
            }
        }
        assert!(moved);
        assert!(t > 0.5, "coast should last a noticeable moment, t={t}");
        assert!(!coast.moving(), "coasting should die out, t={t}");
    }
}
