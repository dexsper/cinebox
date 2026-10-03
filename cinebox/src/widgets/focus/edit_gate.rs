//! Text fields under directional navigation: a plain focus stop until OK is
//! pressed. A focused `TextEdit` takes every arrow key, so it only gets focus
//! while typing.

use egui::{Id, Key, Modifiers, Rect, Response, Sense, Ui};

use super::{directional, track, with_state};

pub struct EditGate {
    /// Pass to `TextEdit::interactive`.
    pub interactive: bool,
    /// The stop that holds focus while not typing; `None` with pointer navigation.
    pub stop: Option<Response>,
}

/// `rect` covers the whole field, `edit_id` is its `TextEdit`'s id.
pub fn edit_gate(ui: &mut Ui, rect: Rect, edit_id: Id) -> EditGate {
    if !directional(ui.ctx()) {
        return EditGate {
            interactive: true,
            stop: None,
        };
    }

    let editing = ui.ctx().memory(|mem| mem.has_focus(edit_id));
    if editing {
        ring_around_field(ui, edit_id, rect);
        return EditGate {
            interactive: true,
            stop: None,
        };
    }

    let stop = ui.interact(rect, field_stop(edit_id), Sense::click());
    track(&stop);

    let start = stop.clicked();
    if start {
        // The OK that opened the field would also end typing in it.
        ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        ui.ctx().memory_mut(|mem| mem.request_focus(edit_id));
        ring_around_field(ui, edit_id, rect);
    }

    EditGate {
        interactive: start,
        stop: Some(stop),
    }
}

/// After the `TextEdit` is shown: typing ended (Enter or Back), so the D-pad
/// continues from the field.
pub fn edit_done(ui: &Ui, edit: &Response) {
    if !directional(ui.ctx()) {
        return;
    }

    if !edit.lost_focus() {
        return;
    }

    let stop = field_stop(edit.id);
    with_state(ui.ctx(), |state| state.return_to = Some(stop));
    ui.ctx().request_repaint();
}

fn ring_around_field(ui: &Ui, edit_id: Id, rect: Rect) {
    with_state(ui.ctx(), |state| state.field_rings.push((edit_id, rect)));
}

/// The stop that stands for the field `edit_id` while it is not being typed in.
#[must_use]
pub fn field_stop(edit_id: Id) -> Id {
    edit_id.with("focus-stop")
}
