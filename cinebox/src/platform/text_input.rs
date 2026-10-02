//! Text fields typed on an on-screen keyboard the OS has to be asked for.
//!
//! Fields [`declare`] how they want to be typed; [`sync`] opens and closes the
//! keyboard as a `TextEdit` gains and loses focus; [`feed`] turns the keyboard's
//! typing into the egui input a `TextEdit` already understands. Without a soft
//! keyboard (desktop) all of it is inert.

use egui::{Context, Event, Id, ImeEvent, Key, Modifiers, RawInput};

use super::{TextInputEvent, TextInputSpec, device};

#[derive(Clone, Default)]
struct State {
    declared: Vec<(Id, TextInputSpec)>,
    /// The `TextEdit` the keyboard is open for.
    open: Option<Id>,
    leave_pending: bool,
}

fn state_id() -> Id {
    Id::new("cinebox-text-input")
}

fn with_state<R>(ctx: &Context, f: impl FnOnce(&mut State) -> R) -> R {
    ctx.data_mut(|data| f(data.get_temp_mut_or_default::<State>(state_id())))
}

/// How the `TextEdit` with `edit_id` is typed; undeclared fields get the default.
/// Call every frame the field is shown.
pub(crate) fn declare(ctx: &Context, edit_id: Id, spec: TextInputSpec) {
    with_state(ctx, |state| state.declared.push((edit_id, spec)));
}

/// After the frame's widgets are shown.
pub(crate) fn sync(ctx: &Context) {
    let declared = with_state(ctx, |state| std::mem::take(&mut state.declared));
    let device = device(ctx);
    if !device.soft_keyboard() {
        return;
    }

    let focused = ctx.memory(|mem| mem.focused());
    let editing = focused.filter(|_| ctx.text_edit_focused());
    let was_open = with_state(ctx, |state| std::mem::replace(&mut state.open, editing));

    if editing == was_open {
        return;
    }

    let Some(edit_id) = editing else {
        device.stop_text_input();
        return;
    };

    let declared = declared.iter().find(|(id, _)| *id == edit_id);
    let spec = declared.map(|(_, spec)| *spec).unwrap_or_default();
    device.start_text_input(spec);
}

/// Before this frame's keyboard events: finish an edit deferred by [`feed`].
pub(crate) fn begin_input(ctx: &Context, raw_input: &mut RawInput) {
    let leave = with_state(ctx, |state| std::mem::take(&mut state.leave_pending));
    if leave {
        tap(raw_input, Key::Escape);
    }
}

/// What the keyboard did, as egui input for the focused `TextEdit`.
pub(crate) fn feed(ctx: &Context, raw_input: &mut RawInput, event: TextInputEvent) {
    let ime = match event {
        TextInputEvent::Preedit(text) => ImeEvent::Preedit {
            text,
            active_range_chars: None,
        },
        TextInputEvent::Commit(text) => ImeEvent::Commit(text),
        TextInputEvent::DeleteSurrounding { before, after } => delete_surrounding(before, after),
        TextInputEvent::Action => return tap(raw_input, Key::Enter),
        TextInputEvent::KeyboardHidden => return leave_field(ctx, raw_input),
    };

    raw_input.events.push(Event::Ime(ime));
}

/// Back already went to the keyboard; finish the edit as Back would. egui
/// drops focus on Escape before widgets see the frame's input, so text typed
/// in the same frame would be lost: then the Escape waits a frame.
fn leave_field(ctx: &Context, raw_input: &mut RawInput) {
    let typed = raw_input.events.iter().any(|event| matches!(event, Event::Ime(_)));
    if !typed {
        tap(raw_input, Key::Escape);
        return;
    }

    with_state(ctx, |state| state.leave_pending = true);
    ctx.request_repaint();
}

fn delete_surrounding(before_chars: usize, after_chars: usize) -> ImeEvent {
    ImeEvent::DeleteSurrounding {
        before_chars,
        after_chars,
    }
}

fn tap(raw_input: &mut RawInput, key: Key) {
    for pressed in [true, false] {
        raw_input.events.push(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    }
}
