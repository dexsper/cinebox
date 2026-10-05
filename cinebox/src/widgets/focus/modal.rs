//! Where focus belongs when it has nowhere sensible to be: inside an open
//! popup, back where it was after the popup closes, or on a starting widget.

use egui::{Context, Id, Key, LayerId, RawInput, Rect};

use super::{Candidate, State};

/// Focus stays on the popup (modal layer) while one is open and returns to
/// where it was once the popup closes.
pub(super) fn keep_in_modal(ctx: &Context, last: &State, next: &mut State) {
    let modal = ctx.memory(|mem| mem.top_modal_layer());
    let focused = ctx.memory(|mem| mem.focused());
    next.had_modal = modal.is_some();
    next.modal = modal;

    let Some(layer) = modal else {
        next.last_allowed = None;
        next.outer_focus.clear();
        next.before_modal = focused;
        restore_after_modal(ctx, last, next, focused);
        return;
    };

    if last.modal != modal {
        let returned = return_to_outer(ctx, last, next, layer);
        if returned {
            return;
        }
    }

    if let Some(id) = focused.filter(|id| allowed(ctx, *id)) {
        next.last_allowed = Some(id);
        return;
    }

    let previous = last.last_allowed.filter(|id| allowed(ctx, *id));
    let Some(target) = previous.or_else(|| starting_point(ctx, last, None)) else {
        return;
    };

    next.last_allowed = Some(target);
    ctx.memory_mut(|mem| mem.request_focus(target));
}

/// A popup over a popup, like a dropdown in a drawer: opening it remembers
/// where focus was in the one below, closing it puts focus back there rather
/// than on the first widget of the one below.
fn return_to_outer(ctx: &Context, last: &State, next: &mut State, layer: LayerId) -> bool {
    let below = next.outer_focus.iter().position(|(outer, _)| *outer == layer);
    let Some(below) = below else {
        if let (Some(outer), Some(id)) = (last.modal, last.last_allowed) {
            next.outer_focus.push((outer, id));
        }

        return false;
    };

    let (_, id) = next.outer_focus[below];
    next.outer_focus.truncate(below);
    if !allowed(ctx, id) {
        return false;
    }

    next.last_allowed = Some(id);
    ctx.memory_mut(|mem| mem.request_focus(id));
    true
}

fn restore_after_modal(ctx: &Context, last: &State, next: &mut State, focused: Option<Id>) {
    if !last.had_modal {
        return;
    }

    let lost = focused.is_none_or(|id| ctx.read_response(id).is_none());
    if !lost {
        return;
    }

    let Some(previous) = last.before_modal else {
        return;
    };

    next.before_modal = Some(previous);
    ctx.memory_mut(|mem| mem.request_focus(previous));
}

/// Nothing focused and the D-pad pressed: land on the first widget of the
/// screen body. The press only places focus; it does not also move or click.
pub(super) fn start_focus(ctx: &Context, last: &State, raw_input: &mut RawInput) {
    let focused = ctx.memory(|mem| mem.focused());
    let lost = focused.is_none_or(|id| ctx.read_response(id).is_none());
    if !lost {
        return;
    }

    let Some(index) = raw_input.events.iter().position(is_nav_press) else {
        return;
    };

    let Some(target) = starting_point(ctx, last, last.content) else {
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

/// The spot the screen or popup asked for, else its first widget inside
/// `area`, else the first one at all.
fn starting_point(ctx: &Context, last: &State, area: Option<Rect>) -> Option<Id> {
    let preferred = last.preferred.filter(|id| allowed(ctx, *id));
    if preferred.is_some() {
        return preferred;
    }

    let in_area = first_allowed(ctx, &last.candidates, area);
    if in_area.is_some() {
        return in_area;
    }

    first_allowed(ctx, &last.candidates, None)
}

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

fn allowed(ctx: &Context, id: Id) -> bool {
    let Some(response) = ctx.read_response(id) else {
        return false;
    };

    ctx.memory(|mem| mem.allows_interaction(response.layer_id))
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
    fn only_dpad_presses_place_focus() {
        assert!(is_nav_press(&key(Key::ArrowDown, true)));
        assert!(is_nav_press(&key(Key::Enter, true)));
        assert!(!is_nav_press(&key(Key::ArrowDown, false)));
        assert!(!is_nav_press(&key(Key::Escape, true)));
    }
}
