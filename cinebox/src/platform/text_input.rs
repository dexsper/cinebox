//! Text fields typed on an on-screen keyboard the OS has to be asked for.
//!
//! Each field reports itself through [`field`]; [`sync`] opens and closes the
//! keyboard as a `TextEdit` gains and loses focus and keeps the keyboard's copy
//! of the field current. The keyboard edits that copy and sends the whole
//! result back, which [`field`] puts into the field. Without a soft keyboard
//! (desktop) all of it is inert.

use egui::text::{CCursor, CCursorRange};
use egui::{Context, Event, Id, Key, Modifiers, RawInput, Response, TextEdit};

use super::{FieldText, TextInputEvent, TextInputSpec, device};

#[derive(Clone, Default)]
struct State {
    /// The fields shown this frame, as they are after this frame's editing.
    shown: Vec<(Id, TextInputSpec, FieldText)>,
    /// The `TextEdit` the keyboard is open for.
    open: Option<Id>,
    /// The keyboard's copy of the open field.
    keyboard_has: Option<FieldText>,
    /// The keyboard's latest edit, not yet put into the field.
    edited: Option<FieldText>,
}

fn state_id() -> Id {
    Id::new("cinebox-text-input")
}

fn with_state<R>(ctx: &Context, f: impl FnOnce(&mut State) -> R) -> R {
    ctx.data_mut(|data| f(data.get_temp_mut_or_default::<State>(state_id())))
}

/// After the field's `TextEdit` is shown, every frame: puts the keyboard's
/// edit into `text` and tells the keyboard how to type the field.
pub(crate) fn field(response: &mut Response, spec: TextInputSpec, text: &mut String) {
    let ctx = response.ctx.clone();
    if !device(&ctx).soft_keyboard() {
        return;
    }

    let edit_id = response.id;
    let edited = with_state(&ctx, |state| take_edit(state, edit_id));
    if let Some(edited) = edited {
        put(&ctx, edit_id, text, &edited);
        response.mark_changed();
    }

    let shown = FieldText {
        text: text.clone(),
        selection: selection(&ctx, edit_id, text),
    };
    with_state(&ctx, |state| state.shown.push((edit_id, spec, shown)));
}

fn take_edit(state: &mut State, edit_id: Id) -> Option<FieldText> {
    if state.open != Some(edit_id) {
        return None;
    }

    state.edited.take()
}

fn put(ctx: &Context, edit_id: Id, text: &mut String, edited: &FieldText) {
    text.clone_from(&edited.text);

    let start = CCursor::new(edited.selection.start);
    let end = CCursor::new(edited.selection.end);
    let mut edit = TextEdit::load_state(ctx, edit_id).unwrap_or_default();
    edit.cursor.set_char_range(Some(CCursorRange::two(start, end)));
    edit.store(ctx, edit_id);
    ctx.request_repaint();
}

fn selection(ctx: &Context, edit_id: Id, text: &str) -> std::ops::Range<usize> {
    let range = TextEdit::load_state(ctx, edit_id).and_then(|edit| edit.cursor.char_range());
    let Some(range) = range else {
        let end = text.chars().count();
        return end..end;
    };

    let [start, end] = range.sorted_cursors();
    start.index.0..end.index.0
}

/// After the frame's widgets are shown.
pub(crate) fn sync(ctx: &Context) {
    let shown = with_state(ctx, |state| std::mem::take(&mut state.shown));
    let device = device(ctx);
    if !device.soft_keyboard() {
        return;
    }

    let focused = ctx.memory(|mem| mem.focused());
    let editing = focused.filter(|_| ctx.text_edit_focused());
    let was_open = with_state(ctx, |state| std::mem::replace(&mut state.open, editing));
    let current = shown.into_iter().find(|(id, _, _)| Some(*id) == editing);

    if editing != was_open {
        with_state(ctx, |state| state.edited = None);
        if editing.is_none() {
            with_state(ctx, |state| state.keyboard_has = None);
            device.stop_text_input();
            return;
        }

        let (spec, field) = match current {
            Some((_, spec, field)) => (spec, field),
            None => (TextInputSpec::default(), FieldText::default()),
        };
        device.start_text_input(spec, &field);
        with_state(ctx, |state| state.keyboard_has = Some(field));
        return;
    }

    let Some((_, _, field)) = current else {
        return;
    };

    let known = with_state(ctx, |state| state.keyboard_has.as_ref() == Some(&field));
    if known {
        return;
    }

    device.update_text_input(&field);
    with_state(ctx, |state| state.keyboard_has = Some(field));
}

/// What the keyboard did, for the open field or as egui input.
pub(crate) fn feed(ctx: &Context, raw_input: &mut RawInput, event: TextInputEvent) {
    match event {
        TextInputEvent::Edited(field) => with_state(ctx, |state| {
            state.keyboard_has = Some(field.clone());
            state.edited = Some(field);
        }),
        TextInputEvent::Action => tap(raw_input, Key::Enter),
        // Back already went to the keyboard; finish the edit as Back would.
        TextInputEvent::KeyboardHidden => tap(raw_input, Key::Escape),
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
