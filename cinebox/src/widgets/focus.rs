//! D-pad focus for the TV form.
//!
//! egui already moves keyboard focus with the arrow keys and clicks the focused
//! widget on Enter. This module fills in what a remote needs on top of that:
//! a visible ring, scrolling the focused widget into view, a starting point when
//! nothing is focused, keeping focus inside popups, and text fields that do not
//! swallow the D-pad. On desktop everything here is inert except [`track`].

use egui::{
    Context, CornerRadius, EventFilter, Id, Key, LayerId, Rect, Response, Sense, Stroke,
    StrokeKind, Ui,
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

#[derive(Clone, Default)]
struct State {
    /// Filled by [`track`] during the frame, read before the next one.
    candidates: Vec<Candidate>,
    /// Widgets that painted their own focus look this frame (see [`lit`]).
    lit: Vec<Id>,
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

/// Pointer over the widget, or the D-pad on it. A widget that uses this paints
/// its own highlight, so the shared focus ring leaves it alone.
#[must_use]
pub fn lit(response: &Response) -> bool {
    let focused = response.has_focus() && platform::is_tv(&response.ctx);
    if focused {
        with_state(&response.ctx, |state| state.lit.push(response.id));
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

/// The arrows act on this widget instead of moving focus off it (seek bars, video).
pub fn hold_arrows(ui: &Ui, id: Id, horizontal: bool, vertical: bool) {
    let filter = EventFilter {
        horizontal_arrows: horizontal,
        vertical_arrows: vertical,
        ..EventFilter::default()
    };

    ui.ctx()
        .memory_mut(|mem| mem.set_focus_lock_filter(id, filter));
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

    let fallback = state
        .last_allowed
        .filter(|id| allowed(ctx, *id))
        .or_else(|| first_allowed(ctx, &state.candidates, None));

    let Some(target) = fallback else {
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

    let target = first_allowed(ctx, &state.candidates, state.content)
        .or_else(|| first_allowed(ctx, &state.candidates, None));
    let Some(target) = target else {
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

/// Ring around the focused widget unless it drew its own highlight. Call last.
pub fn paint_ring(ctx: &Context, theme: &Theme) {
    if !platform::is_tv(ctx) {
        return;
    }

    let Some(id) = ctx.memory(|mem| mem.focused()) else {
        return;
    };

    let drew_own = with_state(ctx, |state| state.lit.contains(&id));
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
