//! Overlay scrollbars, click-drag, Shift+wheel, and inertial coasting.
//!
//! A wheel notch becomes velocity; friction is exponential. Drag stays with
//! egui so nested buttons still receive clicks.

use egui::containers::scroll_area::{DragScroll, ScrollSource};
use egui::{
    AsIdSalt, Direction, Event, Id, Margin, Modifiers, MouseWheelUnit, Pos2, Rect, ScrollArea,
    Shape, Ui, Vec2, Vec2b, pos2, vec2,
};

const FRICTION: f32 = 4.2;
const MIN_SPEED: f32 = 22.0;
const WHEEL_GAIN: f32 = 10.0;
const PIXEL_GAIN: f32 = 6.0;
const MAX_SPEED: f32 = 4200.0;
const WHEEL_TAKEN: &str = "cinebox-wheel-taken";
const BOTTOM_FADE_SIZE: f32 = 56.0;
const BOTTOM_FADE_STRENGTH: f32 = 0.72;
const BOTTOM_FADE_BANDS: i32 = 12;

/// Gutter kept clear on the scrollbar side so the floating bar never sits on top of content.
const SCROLL_GUTTER: i8 = 10;

const EDGE_EPS: f32 = 0.5;

#[derive(Clone, Copy, Debug)]
struct Coast {
    vel: Vec2,
    offset: Vec2,
    rect: Rect,
    dragging: bool,
    max_offset: Vec2,
}

impl Default for Coast {
    fn default() -> Self {
        Self {
            vel: Vec2::ZERO,
            offset: Vec2::ZERO,
            rect: Rect::NOTHING,
            dragging: false,
            max_offset: Vec2::splat(f32::INFINITY),
        }
    }
}

impl Coast {
    fn impulse(&mut self, dvel: Vec2, enabled: Vec2b) {
        for d in 0..2 {
            if !enabled[d] || dvel[d] == 0.0 {
                continue;
            }
            if self.blocked(d, dvel[d]) {
                continue;
            }
            self.vel[d] = (self.vel[d] + dvel[d]).clamp(-MAX_SPEED, MAX_SPEED);
        }
    }

    fn blocked(&self, axis: usize, vel: f32) -> bool {
        let max = self.max_offset[axis].max(0.0);
        (vel < 0.0 && self.offset[axis] <= EDGE_EPS)
            || (vel > 0.0 && self.offset[axis] >= max - EDGE_EPS)
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

    /// Keep offset inside the content and kill velocity into a wall.
    fn clamp_edges(&mut self, enabled: Vec2b) {
        for d in 0..2 {
            if !enabled[d] {
                continue;
            }
            let max = self.max_offset[d].max(0.0);
            if self.offset[d] <= EDGE_EPS {
                self.offset[d] = 0.0;
                if self.vel[d] < 0.0 {
                    self.vel[d] = 0.0;
                }
            }
            if self.offset[d] >= max - EDGE_EPS {
                self.offset[d] = max;
                if self.vel[d] > 0.0 {
                    self.vel[d] = 0.0;
                }
            }
        }
    }
}

/// Keeps rows away from the floating scrollbar's edge without shrinking the hit area.
fn gutter_margin(enabled: Vec2b) -> Margin {
    Margin {
        right: if enabled[1] { SCROLL_GUTTER } else { 0 },
        bottom: if enabled[0] { SCROLL_GUTTER } else { 0 },
        ..Margin::ZERO
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
    show(ui, id, Vec2b::new(false, true), Vec2b::FALSE, None, false, add);
}

/// Like [`vertical`], but this frame starts at the top (new title, re-entry).
pub fn vertical_to_top(ui: &mut Ui, id: impl AsIdSalt, add: impl FnOnce(&mut Ui)) {
    show(ui, id, Vec2b::new(false, true), Vec2b::FALSE, None, true, add);
}

/// Horizontal shelf: height follows content.
pub fn horizontal(ui: &mut Ui, id: impl AsIdSalt, add: impl FnOnce(&mut Ui)) {
    show(
        ui,
        id,
        Vec2b::new(true, false),
        Vec2b::new(false, true),
        None,
        false,
        add,
    );
}

fn show(
    ui: &mut Ui,
    salt: impl AsIdSalt,
    enabled: Vec2b,
    auto_shrink: Vec2b,
    max_height: Option<f32>,
    to_top: bool,
    add: impl FnOnce(&mut Ui),
) {
    let coast_id = ui.id().with(("coast", &salt));
    let mut coast: Coast = ui.ctx().data(|d| d.get_temp(coast_id)).unwrap_or_default();

    if to_top {
        coast.offset = Vec2::ZERO;
        coast.stop();
    }

    let pointer_down = ui.input(|i| i.pointer.primary_down());
    if coast.dragging || (pointer_down && pointer_over(ui, coast.rect)) {
        coast.stop();
    }

    let coasting = coast.moving();
    if coasting && !pointer_down {
        let dt = ui.input(|i| i.stable_dt);
        let delta = coast.step(dt);
        coast.offset += delta;
        coast.clamp_edges(enabled);
    }

    let mut area = ScrollArea::new(enabled)
        .id_salt(salt)
        .auto_shrink(auto_shrink)
        .scroll_source(source())
        .content_margin(gutter_margin(enabled));

    if let Some(height) = max_height {
        area = area.max_height(height);
    }

    if to_top {
        if enabled[0] {
            area = area.horizontal_scroll_offset(0.0);
        }

        if enabled[1] {
            area = area.vertical_scroll_offset(0.0);
        }
    } else if coasting {
        if enabled[0] {
            area = area.horizontal_scroll_offset(coast.offset.x);
        }
        if enabled[1] {
            area = area.vertical_scroll_offset(coast.offset.y);
        }
    }

    let origin = ui.cursor().min;
    let output = area.show(ui, add);
    let hit = hover_rect(
        origin,
        ui.cursor().min,
        ui.spacing().item_spacing,
        output.inner_rect,
        output.content_size,
        enabled,
        ui.clip_rect(),
    );

    if enabled[1] {
        paint_bottom_fade(
            ui,
            output.inner_rect,
            output.content_size,
            output.state.offset,
        );
    }

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
    coast.max_offset = vec2(
        (output.content_size.x - output.inner_rect.width()).max(0.0),
        (output.content_size.y - output.inner_rect.height()).max(0.0),
    );

    coast.clamp_edges(enabled);
    if coast.moving() {
        ui.ctx().request_repaint();
    }

    ui.ctx().data_mut(|d| d.insert_temp(coast_id, coast));
}

fn paint_bottom_fade(ui: &Ui, inner: Rect, content: Vec2, offset: Vec2) {
    let overflow = content.y - inner.height();
    let distance = overflow - offset.y;
    if distance <= 0.0 {
        return;
    }

    let peak = (distance / BOTTOM_FADE_SIZE).clamp(0.0, 1.0) * BOTTOM_FADE_STRENGTH;
    let bg = ui.visuals().panel_fill;
    let fade = Rect::from_min_max(
        pos2(inner.left(), inner.bottom() - BOTTOM_FADE_SIZE),
        inner.right_bottom(),
    );

    let n = BOTTOM_FADE_BANDS as f32;
    for i in 0..BOTTOM_FADE_BANDS {
        let a0 = i as f32 / n;
        let a1 = (i + 1) as f32 / n;
        let c0 = bg.gamma_multiply(peak * a0 * a0);
        let c1 = bg.gamma_multiply(peak * a1 * a1);
        let y0 = fade.top() + fade.height() * a0;
        let y1 = fade.top() + fade.height() * a1;
        let band = Rect::from_min_max(pos2(fade.left(), y0), pos2(fade.right(), y1));
        
        ui.painter()
            .add(Shape::gradient_rect(band, Direction::TopDown, [c0, c1]));
    }
}

/// Visible hover target from the parent layout band, not `inner_rect`.
///
/// Nested `ScrollArea::inner_rect` can follow a clipped `available_rect` after
/// the parent has scrolled, so the first shelves keep covering the viewport.
fn hover_rect(
    origin: Pos2,
    cursor_after: Pos2,
    spacing: Vec2,
    inner: Rect,
    content: Vec2,
    enabled: Vec2b,
    clip: Rect,
) -> Rect {
    // Do not min() with inner.size(): after the parent scrolls, inner_rect can be
    // the clipped viewport, so the first shelves would keep covering it.
    let mut bottom_right = pos2(origin.x + inner.width(), cursor_after.y - spacing.y);
    if !enabled[0] {
        bottom_right.x = origin.x + content.x.min(inner.width().max(1.0));
    }

    if !enabled[1] {
        bottom_right.y = origin.y + content.y;
    }

    if bottom_right.y < origin.y {
        bottom_right.y = origin.y;
    }

    Rect::from_min_max(origin, bottom_right).intersect(clip)
}

fn pointer_over(ui: &Ui, rect: Rect) -> bool {
    rect.is_positive() && ui.rect_contains_pointer(rect)
}

/// Viewport on scroll axes; content size on the rest so a shelf cannot cover rows below it.
#[cfg(test)]
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
        coast.impulse(dvel, enabled);
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
        let Some(dvel) = wheel_impulse(
            [true, false],
            MouseWheelUnit::Line,
            vec2(0.0, 1.0),
            Modifiers::SHIFT,
        ) else {
            panic!("shift+wheel on a row");
        };
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
    fn hover_rect_uses_layout_origin_not_clipped_inner() {
        let clip = Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 220.0));
        let spacing = vec2(8.0, 8.0);
        let clipped_inner = Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 600.0));
        let content = vec2(4000.0, 72.0);
        let enabled = Vec2b::new(true, false);

        let offscreen = hover_rect(
            pos2(0.0, -180.0),
            pos2(0.0, -100.0),
            spacing,
            clipped_inner,
            content,
            enabled,
            clip,
        );
        assert!(
            !offscreen.contains(pos2(80.0, 40.0)),
            "off-screen shelf must not steal the viewport, hit={offscreen:?}"
        );

        let visible = hover_rect(
            pos2(0.0, 40.0),
            pos2(0.0, 120.0),
            spacing,
            clipped_inner,
            content,
            enabled,
            clip,
        );
        assert!(
            visible.contains(pos2(80.0, 70.0)),
            "visible lower shelf should receive the pointer, hit={visible:?}"
        );
        assert!(
            !visible.contains(pos2(80.0, 20.0)),
            "visible shelf must not cover the viewport top, hit={visible:?}"
        );

        let squeezed_inner = Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 10.0));
        let squeezed = hover_rect(
            pos2(0.0, 40.0),
            pos2(0.0, 50.0),
            spacing,
            squeezed_inner,
            content,
            enabled,
            clip,
        );
        assert!(
            squeezed.contains(pos2(80.0, 70.0)),
            "clipped inner height must not shrink the shelf hit, hit={squeezed:?}"
        );
    }

    #[test]
    fn inertia_decays_and_stops() {
        let mut coast = Coast {
            max_offset: vec2(0.0, 800.0),
            ..Coast::default()
        };
        coast.impulse(vec2(0.0, 800.0), Vec2b::new(false, true));
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

    #[test]
    fn coast_stops_at_bottom_instead_of_overshooting() {
        let mut coast = Coast {
            offset: vec2(0.0, 198.0),
            vel: vec2(0.0, 900.0),
            max_offset: vec2(0.0, 200.0),
            ..Coast::default()
        };
        let delta = coast.step(1.0 / 60.0);
        coast.offset += delta;
        coast.clamp_edges(Vec2b::new(false, true));
        assert_eq!(coast.offset.y, 200.0);
        assert_eq!(coast.vel.y, 0.0);
        assert!(!coast.moving());
    }

    #[test]
    fn coast_stops_at_top_instead_of_overshooting() {
        let mut coast = Coast {
            offset: vec2(0.0, 2.0),
            vel: vec2(0.0, -900.0),
            max_offset: vec2(0.0, 200.0),
            ..Coast::default()
        };
        let delta = coast.step(1.0 / 60.0);
        coast.offset += delta;
        coast.clamp_edges(Vec2b::new(false, true));
        assert_eq!(coast.offset.y, 0.0);
        assert_eq!(coast.vel.y, 0.0);
    }

    #[test]
    fn wheel_into_a_wall_is_ignored() {
        let mut coast = Coast {
            offset: Vec2::ZERO,
            max_offset: vec2(0.0, 200.0),
            ..Coast::default()
        };
        coast.impulse(vec2(0.0, -800.0), Vec2b::new(false, true));
        assert!(!coast.moving());

        coast.offset = vec2(0.0, 200.0);
        coast.impulse(vec2(0.0, 800.0), Vec2b::new(false, true));
        assert!(!coast.moving());

        coast.impulse(vec2(0.0, -800.0), Vec2b::new(false, true));
        assert!(coast.moving());
        assert!(coast.vel.y < 0.0);
    }
}
