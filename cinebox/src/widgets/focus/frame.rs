//! The two per-frame hooks the app calls around building its UI.

use egui::{Context, RawInput};

use super::{State, directional, modal, navigate, ring, with_state};
use crate::theme::Theme;

/// Before egui sees the frame's input (`raw_input_hook`): settle where focus
/// should be, using what the previous frame registered.
pub fn begin_frame(ctx: &Context, raw_input: &mut RawInput) {
    if !directional(ctx) {
        return;
    }

    let last = with_state(ctx, std::mem::take);
    let mut next = State {
        content: last.content,
        before_modal: last.before_modal,
        had_modal: last.had_modal,
        outer_focus: last.outer_focus.clone(),
        last_allowed: last.last_allowed,
        was_editing: ctx.text_edit_focused(),
        focused_before_input: ctx.memory(|mem| mem.focused()),
        revealed: last.revealed,
        ..State::default()
    };

    if let Some(stop) = last.return_to {
        ctx.memory_mut(|mem| mem.request_focus(stop));
    }

    modal::keep_in_modal(ctx, &last, &mut next);
    modal::start_focus(ctx, &last, raw_input);

    with_state(ctx, |live| *live = next);
}

/// After every widget of the frame is shown.
pub fn end_frame(ctx: &Context, theme: &Theme) {
    if !directional(ctx) {
        return;
    }

    let state = with_state(ctx, |state| state.clone());
    ring::paint(ctx, theme, &state);
    navigate::step(ctx, &state);
}
