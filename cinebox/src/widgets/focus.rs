//! D-pad focus for the TV form.
//!
//! egui clicks the focused widget on Enter. This module adds what a remote needs
//! on top of that: moving focus with the arrows between the widgets passed to
//! [`track`], a visible ring, scrolling the focused widget into view, a starting
//! point when nothing is focused, keeping focus inside popups, and text fields
//! that do not swallow the D-pad. On desktop everything here is inert except
//! [`track`].

use egui::{
    Context, CornerRadius, EventFilter, FocusDirection, Id, Key, LayerId, Rangef, Rect, Response,
    Sense, Stroke, StrokeKind, Ui, Vec2, vec2,
};

use crate::platform;
use crate::theme::Theme;

/// A focusable widget seen this frame, in creation (reading) order.
#[derive(Clone, Copy)]
struct Candidate {
    id: Id,
    rect: Rect,
    layer: LayerId,
}

/// A widget that takes some arrows itself while focused (see [`hold_arrows`]).
#[derive(Clone, Copy)]
struct Hold {
    id: Id,
    horizontal: bool,
    vertical: bool,
}

impl Hold {
    /// `widget` is this one and keeps arrows pointing `toward` for itself.
    fn keeps(&self, widget: Id, toward: Vec2) -> bool {
        if self.id != widget {
            return false;
        }

        if toward.x != 0.0 {
            return self.horizontal;
        }

        self.vertical
    }
}

const DIRECTIONS: [(Key, Vec2); 4] = [
    (Key::ArrowUp, Vec2::UP),
    (Key::ArrowDown, Vec2::DOWN),
    (Key::ArrowLeft, Vec2::LEFT),
    (Key::ArrowRight, Vec2::RIGHT),
];

#[derive(Clone, Default)]
struct State {
    /// Filled by [`track`] during the frame, read before the next one.
    candidates: Vec<Candidate>,
    /// Starting focus a screen or popup asked for this frame (see [`prefer`]).
    preferred: Option<Id>,
    /// Filled by [`hold_arrows`] during the frame.
    holds: Vec<Hold>,
    /// Widgets that drew their own focus ring this frame (see [`own_ring`]).
    own_rings: Vec<Id>,
    /// Where the next starting focus should land when nothing is focused.
    content: Option<Rect>,
    /// Focus outside the popup when it opened; given back when it closes.
    before_modal: Option<Id>,
    had_modal: bool,
    /// Last focus that was allowed under the current modal layer.
    last_allowed: Option<Id>,
    /// A text field was being edited when the frame's input arrived.
    was_editing: bool,
}

fn state_id() -> Id {
    Id::new("cinebox-focus")
}

fn with_state<R>(ctx: &Context, f: impl FnOnce(&mut State) -> R) -> R {
    ctx.data_mut(|data| f(data.get_temp_mut_or_default::<State>(state_id())))
}

/// Pointer over the widget, or the D-pad on it: when a widget that paints its
/// own background shows the hover look. On TV [`paint_ring`] still marks focus.
#[must_use]
pub fn lit(response: &Response) -> bool {
    let focused = response.has_focus() && platform::is_tv(&response.ctx);
    focused || response.hovered()
}

/// Like [`lit`], for a widget that draws its own ring (a poster's), so
/// [`paint_ring`] does not add a second one.
#[must_use]
pub fn own_ring(response: &Response) -> bool {
    let focused = response.has_focus() && platform::is_tv(&response.ctx);
    if focused {
        with_state(&response.ctx, |state| state.own_rings.push(response.id));
        return true;
    }

    response.hovered()
}

/// Register a clickable: scroll it into view when it gains focus and offer it as
/// a starting point. Must run where the widget is created (inside its scroll area).
pub fn track(response: &Response) {
    if response.gained_focus() {
        response.scroll_to_me(None);
    }

    if !platform::is_tv(&response.ctx) {
        return;
    }

    let focusable = response.sense.is_focusable() && response.enabled();
    if !focusable {
        return;
    }

    let candidate = Candidate {
        id: response.id,
        rect: response.rect,
        layer: response.layer_id,
    };
    with_state(&response.ctx, |state| state.candidates.push(candidate));
}

/// Keep the D-pad inside this popup while it is open.
pub fn trap(ui: &Ui) {
    if !platform::is_tv(ui.ctx()) {
        return;
    }

    let layer = ui.layer_id();
    ui.ctx().memory_mut(|mem| mem.set_modal_layer(layer));

    // Focus moves in before the next frame (see `keep_in_modal`), so make sure there is one.
    if focused_layer(ui.ctx()) != Some(layer) {
        ui.ctx().request_repaint();
    }
}

/// Put the D-pad on this item of a popup that just opened.
pub fn enter_popup(response: &Response) {
    if platform::is_tv(&response.ctx) {
        response.request_focus();
    }
}

/// Where focus should start in this popup or screen instead of its first widget
/// (the current choice, the main action). The first call in a frame wins.
pub fn prefer(response: &Response) {
    if !platform::is_tv(&response.ctx) {
        return;
    }

    let id = response.id;
    with_state(&response.ctx, |state| {
        state.preferred.get_or_insert(id);
    });
}

/// The arrows act on this widget instead of moving focus off it (seek bars, video).
pub fn hold_arrows(ui: &Ui, id: Id, horizontal: bool, vertical: bool) {
    let filter = EventFilter {
        horizontal_arrows: horizontal,
        vertical_arrows: vertical,
        ..EventFilter::default()
    };

    ui.ctx()
        .memory_mut(|mem| mem.set_focus_lock_filter(id, filter));

    let hold = Hold {
        id,
        horizontal,
        vertical,
    };
    with_state(ui.ctx(), |state| state.holds.push(hold));
}

/// Move focus with the D-pad. Call after every widget of the frame is shown.
///
/// egui moves focus itself, but it also lands on widgets under an open popup
/// (it remembers them from before the popup opened). This walks only this
/// frame's focus stops on layers that take input, the way egui scores them.
pub fn navigate(ctx: &Context) {
    if !platform::is_tv(ctx) {
        return;
    }

    if ctx.text_edit_focused() {
        return;
    }

    let Some(toward) = pressed_direction(ctx) else {
        return;
    };

    let Some(focused) = ctx.memory(|mem| mem.focused()) else {
        return;
    };

    let state = with_state(ctx, |state| state.clone());
    let held = state.holds.iter().any(|hold| hold.keeps(focused, toward));
    if held {
        return;
    }

    let Some(from) = ctx.read_response(focused) else {
        return;
    };

    // egui would make its own move at the end of the pass.
    ctx.memory_mut(|mem| mem.move_focus(FocusDirection::None));
    ctx.request_repaint();

    let Some(target) = nearest(ctx, from.rect, toward, &state.candidates) else {
        return;
    };

    ctx.memory_mut(|mem| mem.request_focus(target));
}

fn pressed_direction(ctx: &Context) -> Option<Vec2> {
    for (key, toward) in DIRECTIONS {
        let pressed = ctx.input(|i| i.key_pressed(key));
        if pressed {
            return Some(toward);
        }
    }

    None
}

/// The candidate closest to `from` within 45° of `toward`. Overlapping spans
/// count as aligned, so the list item below beats a button off to the side.
fn nearest(ctx: &Context, from: Rect, toward: Vec2, candidates: &[Candidate]) -> Option<Id> {
    let mut best: Option<(Id, f32)> = None;

    for candidate in candidates {
        let interactive = ctx.memory(|mem| mem.allows_interaction(candidate.layer));
        if !interactive {
            continue;
        }

        let dx = span_offset(candidate.rect.x_range(), from.x_range());
        let dy = span_offset(candidate.rect.y_range(), from.y_range());
        let offset = vec2(dx, dy);

        // Zero for the focused widget itself, so it never qualifies.
        let alignment = offset.normalized().dot(toward);
        if alignment < std::f32::consts::FRAC_1_SQRT_2 {
            continue;
        }

        let score = offset.length() / (alignment * alignment);
        let better = best.is_none_or(|(_, best_score)| score < best_score);
        if better {
            best = Some((candidate.id, score));
        }
    }

    best.map(|(id, _)| id)
}

/// Zero when the spans overlap by at least half the shorter one, else the
/// distance between their centers (negative when `a` comes first).
fn span_offset(a: Rangef, b: Rangef) -> f32 {
    let overlap = a.intersection(b).span();
    let shorter = a.span().min(b.span());
    if overlap >= shorter * 0.5 {
        return 0.0;
    }

    a.center() - b.center()
}

/// The focused widget's layer, from the previous frame.
#[must_use]
pub fn focused_layer(ctx: &Context) -> Option<LayerId> {
    let id = ctx.memory(|mem| mem.focused())?;

    ctx.read_response(id).map(|response| response.layer_id)
}

/// Where the starting focus should land: the screen body, not the title bar or rail.
pub fn set_content(ctx: &Context, rect: Rect) {
    with_state(ctx, |state| state.content = Some(rect));
}

/// A text field was focused when this frame's input arrived, so Escape (the
/// remote's Back) only leaves the field.
#[must_use]
pub fn was_editing(ctx: &Context) -> bool {
    with_state(ctx, |state| state.was_editing)
}

/// Runs before egui sees the frame's input (`raw_input_hook`).
pub fn before_pass(ctx: &Context, raw_input: &mut egui::RawInput) {
    if !platform::is_tv(ctx) {
        return;
    }

    back_is_escape(raw_input);

    let state = with_state(ctx, std::mem::take);
    let mut next = State {
        content: state.content,
        before_modal: state.before_modal,
        had_modal: state.had_modal,
        last_allowed: state.last_allowed,
        was_editing: ctx.text_edit_focused(),
        ..State::default()
    };

    keep_in_modal(ctx, &state, &mut next);
    start_focus(ctx, &state, raw_input);

    with_state(ctx, |live| *live = next);
}

/// The remote's Back arrives as `BrowserBack`; everything (popups, text
/// fields, navigation) already reacts to Escape.
fn back_is_escape(raw_input: &mut egui::RawInput) {
    for event in &mut raw_input.events {
        let egui::Event::Key { key, .. } = event else {
            continue;
        };

        if *key == Key::BrowserBack {
            *key = Key::Escape;
        }
    }
}

fn allowed(ctx: &Context, id: Id) -> bool {
    let Some(response) = ctx.read_response(id) else {
        return false;
    };

    ctx.memory(|mem| mem.allows_interaction(response.layer_id))
}

/// Focus stays on the popup (modal layer) while one is open and returns to
/// where it was once the popup closes.
fn keep_in_modal(ctx: &Context, state: &State, next: &mut State) {
    let modal = ctx.memory(|mem| mem.top_modal_layer());
    let focused = ctx.memory(|mem| mem.focused());
    next.had_modal = modal.is_some();

    if modal.is_none() {
        next.last_allowed = None;
        next.before_modal = focused;

        if !state.had_modal {
            return;
        }

        let lost = focused.is_none_or(|id| ctx.read_response(id).is_none());
        if !lost {
            return;
        }

        let Some(previous) = state.before_modal else {
            return;
        };

        next.before_modal = Some(previous);
        ctx.memory_mut(|mem| mem.request_focus(previous));
        return;
    }

    if let Some(id) = focused.filter(|id| allowed(ctx, *id)) {
        next.last_allowed = Some(id);
        return;
    }

    let previous = state.last_allowed.filter(|id| allowed(ctx, *id));
    let Some(target) = previous.or_else(|| starting_point(ctx, state, None)) else {
        return;
    };

    next.last_allowed = Some(target);
    ctx.memory_mut(|mem| mem.request_focus(target));
}

/// Nothing focused and the D-pad pressed: land on the first widget of the
/// screen body. The press only places focus; it does not also move or click.
fn start_focus(ctx: &Context, state: &State, raw_input: &mut egui::RawInput) {
    let focused = ctx.memory(|mem| mem.focused());
    let lost = focused.is_none_or(|id| ctx.read_response(id).is_none());
    if !lost {
        return;
    }

    let Some(index) = raw_input.events.iter().position(is_nav_press) else {
        return;
    };

    let Some(target) = starting_point(ctx, state, state.content) else {
        return;
    };

    raw_input.events.remove(index);
    ctx.memory_mut(|mem| mem.request_focus(target));
}

fn is_nav_press(event: &egui::Event) -> bool {
    let egui::Event::Key {
        key, pressed: true, ..
    } = event
    else {
        return false;
    };

    matches!(
        key,
        Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight | Key::Enter
    )
}

/// Where focus lands when it has nowhere to be: the spot the screen or popup
/// asked for, else its first widget inside `area`, else the first one at all.
fn starting_point(ctx: &Context, state: &State, area: Option<Rect>) -> Option<Id> {
    let preferred = state.preferred.filter(|id| allowed(ctx, *id));
    if preferred.is_some() {
        return preferred;
    }

    let in_area = first_allowed(ctx, &state.candidates, area);
    if in_area.is_some() {
        return in_area;
    }

    first_allowed(ctx, &state.candidates, None)
}

/// First visible candidate on an interactive layer, optionally inside `area`.
fn first_allowed(ctx: &Context, candidates: &[Candidate], area: Option<Rect>) -> Option<Id> {
    let screen = ctx.content_rect();
    let found = candidates.iter().find(|candidate| {
        let on_screen = screen.contains_rect(candidate.rect);
        let in_area = area.is_none_or(|area| area.contains(candidate.rect.center()));
        let interactive = ctx.memory(|mem| mem.allows_interaction(candidate.layer));

        on_screen && in_area && interactive
    });

    found.map(|candidate| candidate.id)
}

/// Ring around the focused widget, the one focus mark a remote user looks for.
/// Call after every widget is shown.
pub fn paint_ring(ctx: &Context, theme: &Theme) {
    if !platform::is_tv(ctx) {
        return;
    }

    let Some(id) = ctx.memory(|mem| mem.focused()) else {
        return;
    };

    let drew_own = with_state(ctx, |state| state.own_rings.contains(&id));
    if drew_own {
        return;
    }

    let Some(response) = ctx.read_response(id) else {
        return;
    };

    let visible = response.interact_rect;
    if visible.width() < 1.0 || visible.height() < 1.0 {
        return;
    }

    let ring = response.rect.expand(theme.ring_gap);
    let radius = CornerRadius::same(theme.radius_card.round() as u8);
    ctx.layer_painter(response.layer_id)
        .with_clip_rect(visible.expand(theme.ring_w + theme.ring_gap))
        .rect_stroke(
            ring,
            radius,
            Stroke::new(theme.ring_w, theme.ring),
            StrokeKind::Outside,
        );
}

/// A text field on TV: a plain focus stop until OK is pressed. A focused
/// `TextEdit` takes every arrow key, so it only gets focus while typing.
pub struct EditGate {
    /// Pass to `TextEdit::interactive`.
    pub interactive: bool,
    /// The stop that holds D-pad focus while not typing; `None` on desktop.
    pub stop: Option<Response>,
}

/// `rect` covers the field, `edit_id` is the `TextEdit`'s id.
pub fn edit_gate(ui: &mut Ui, rect: Rect, edit_id: Id) -> EditGate {
    if !platform::is_tv(ui.ctx()) {
        return EditGate {
            interactive: true,
            stop: None,
        };
    }

    let editing = ui.ctx().memory(|mem| mem.has_focus(edit_id));
    if editing {
        return EditGate {
            interactive: true,
            stop: None,
        };
    }

    let stop = ui.interact(rect, stop_id(edit_id), Sense::click());
    track(&stop);

    let start = stop.clicked();
    if start {
        // The OK that opened the field would also end typing in it.
        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
        ui.ctx().memory_mut(|mem| mem.request_focus(edit_id));
    }

    EditGate {
        interactive: start,
        stop: Some(stop),
    }
}

/// After the `TextEdit` is shown: typing ended (Enter or Back), so the D-pad
/// continues from the field.
pub fn edit_done(ui: &Ui, edit: &Response) {
    if !platform::is_tv(ui.ctx()) || !edit.lost_focus() {
        return;
    }

    let stop = stop_id(edit.id);
    ui.ctx().memory_mut(|mem| mem.request_focus(stop));
}

fn stop_id(edit_id: Id) -> Id {
    edit_id.with("tv-stop")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn back_becomes_escape_both_ways() {
        let mut raw = egui::RawInput {
            events: vec![key(Key::BrowserBack, true), key(Key::BrowserBack, false)],
            ..Default::default()
        };

        back_is_escape(&mut raw);

        assert!(raw.events.iter().all(|event| matches!(
            event,
            egui::Event::Key {
                key: Key::Escape,
                ..
            }
        )));
    }

    #[test]
    fn only_dpad_presses_place_focus() {
        assert!(is_nav_press(&key(Key::ArrowDown, true)));
        assert!(is_nav_press(&key(Key::Enter, true)));
        assert!(!is_nav_press(&key(Key::ArrowDown, false)));
        assert!(!is_nav_press(&key(Key::Escape, true)));
    }
}
