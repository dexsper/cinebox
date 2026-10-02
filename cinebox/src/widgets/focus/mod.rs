//! Directional (D-pad) focus.
//!
//! egui clicks the focused widget on Enter; this adds what a remote needs on
//! top: moving focus with the arrows between the widgets passed to [`track`],
//! a visible ring, scrolling the focused widget into view, a starting point
//! when nothing is focused, keeping focus inside popups, and text fields that
//! do not swallow the D-pad. With pointer navigation everything here is inert
//! except the scroll in [`track`].

mod edit_gate;
mod frame;
mod modal;
mod navigate;
mod ring;

use egui::{Context, EventFilter, Id, LayerId, PointerButton, Rect, Response, Ui};

use crate::platform::{self, UiSound};

pub use edit_gate::{edit_done, edit_gate};
pub use frame::{begin_frame, end_frame};

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

/// See [`group`].
#[derive(Clone)]
struct Group {
    members: Vec<Id>,
    entry: Id,
}

#[derive(Clone, Default)]
struct State {
    /// Filled while the frame is built; [`begin_frame`] keeps last frame's for its decisions.
    candidates: Vec<Candidate>,
    preferred: Option<Id>,
    holds: Vec<Hold>,
    groups: Vec<Group>,
    own_marks: Vec<Id>,
    /// Text fields being edited: the ring goes around the whole field, not its inner `TextEdit`.
    field_rings: Vec<(Id, Rect)>,
    /// A field's stop to refocus once its typing ended; the stop only exists
    /// from the next frame on.
    return_to: Option<Id>,
    content: Option<Rect>,
    before_modal: Option<Id>,
    had_modal: bool,
    last_allowed: Option<Id>,
    was_editing: bool,
}

fn state_id() -> Id {
    Id::new("cinebox-focus")
}

fn with_state<R>(ctx: &Context, f: impl FnOnce(&mut State) -> R) -> R {
    ctx.data_mut(|data| f(data.get_temp_mut_or_default::<State>(state_id())))
}

fn directional(ctx: &Context) -> bool {
    platform::profile(ctx).is_directional()
}

/// Pointer over the widget, or the D-pad on it: when a widget that paints its
/// own background shows the hover look. The focus ring is painted on top.
#[must_use]
pub fn lit(response: &Response) -> bool {
    let focused = response.has_focus() && directional(&response.ctx);
    focused || response.hovered()
}

/// Like [`lit`], for a widget that shows focus its own way (a poster's ring,
/// a side menu item's fill), so the shared ring is not added on top.
#[must_use]
pub fn own_mark(response: &Response) -> bool {
    let focused = response.has_focus() && directional(&response.ctx);
    if focused {
        with_state(&response.ctx, |state| state.own_marks.push(response.id));
        return true;
    }

    response.hovered()
}

/// Register a clickable as a focus stop. Must run where the widget is created
/// (inside its scroll area), so gaining focus can scroll it into view.
pub fn track(response: &Response) {
    if response.gained_focus() {
        response.scroll_to_me(None);
    }

    if !directional(&response.ctx) {
        return;
    }

    let focusable = response.sense.is_focusable() && response.enabled();
    if !focusable {
        return;
    }

    let by_remote = response.clicked() && !response.clicked_by(PointerButton::Primary);
    if by_remote {
        platform::device(&response.ctx).play_sound(UiSound::Activate);
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
    if !directional(ui.ctx()) {
        return;
    }

    let layer = ui.layer_id();
    ui.ctx().memory_mut(|mem| mem.set_modal_layer(layer));

    // Focus moves in at the start of the next frame, so there has to be one.
    if focused_layer(ui.ctx()) != Some(layer) {
        ui.ctx().request_repaint();
    }
}

/// Put the D-pad on this item of a popup that just opened.
pub fn enter_popup(response: &Response) {
    if directional(&response.ctx) {
        response.request_focus();
    }
}

/// Where focus should start in this popup or screen instead of its first widget
/// (the current choice, the main action). The first call in a frame wins.
pub fn prefer(response: &Response) {
    if !directional(&response.ctx) {
        return;
    }

    let id = response.id;
    with_state(&response.ctx, |state| {
        state.preferred.get_or_insert(id);
    });
}

/// Stops entered as one, like a side menu: arriving from outside lands on
/// `entry` (the current choice) rather than on whichever member is nearest.
pub fn group(ctx: &Context, members: Vec<Id>, entry: Id) {
    if !directional(ctx) {
        return;
    }

    with_state(ctx, |state| state.groups.push(Group { members, entry }));
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
