//! Single-line text input in the settings style, with commit-on-settle semantics.

use std::time::Duration;

use egui::{Align, Frame, Id, Layout, Margin, Sense, Stroke, TextEdit, Ui, UiBuilder, vec2};

use crate::platform::{self, TextAction, TextInputSpec, TextPurpose};
use crate::theme::Theme;
use crate::widgets::focus;

pub const INPUT_H: f32 = crate::widgets::button::CONTROL_H;
pub const COMMIT_IDLE: f64 = 0.8;

/// Text typed into a [`committed_edit`], kept in egui memory between frames.
#[derive(Clone)]
struct Draft {
    text: String,
    /// Time of the last keystroke not handed back yet; `None` once committed.
    edited_at: Option<f64>,
}

impl Draft {
    fn clean(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            edited_at: None,
        }
    }

    fn is_pending(&self) -> bool {
        self.edited_at.is_some()
    }

    /// Seconds until the idle commit; `None` when nothing is pending.
    fn idle_left(&self, now: f64) -> Option<f64> {
        let edited_at = self.edited_at?;
        Some((COMMIT_IDLE - (now - edited_at)).max(0.0))
    }
}

/// Typing edits a draft; the draft is handed back only on Enter, focus loss,
/// or after [`COMMIT_IDLE`] seconds without a keystroke. Keeps settings,
/// disk writes, and the network quiet while a key or URL is being typed.
pub fn committed_edit(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    value: &str,
    placeholder: &str,
    purpose: TextPurpose,
) -> Option<String> {
    let now = ui.input(|i| i.time);
    let stored = ui.data(|d| d.get_temp::<Draft>(id));
    let mut draft = stored.unwrap_or_else(|| Draft::clean(value));

    let edit_id = id.with("edit");
    let response = styled_edit(ui, theme, edit_id, &mut draft.text, placeholder, purpose);
    if response.changed() {
        draft.edited_at = Some(now);
    }

    let idle_left = draft.idle_left(now);
    let settled = idle_left.is_some_and(|left| left <= 0.0);
    let commit = draft.is_pending() && (response.lost_focus() || settled);

    if commit {
        draft.edited_at = None;
    } else if let Some(left) = idle_left {
        let wait = Duration::from_secs_f64(left);
        ui.ctx().request_repaint_after(wait);
    }

    let committed = commit.then(|| draft.text.clone());
    store_draft(ui, id, draft, response.has_focus());
    committed
}

/// Keep the draft while the field is focused or has edits not handed back;
/// otherwise drop it so the field shows the stored value again.
fn store_draft(ui: &Ui, id: Id, draft: Draft, focused: bool) {
    if focused || draft.is_pending() {
        ui.data_mut(|d| d.insert_temp(id, draft));
        return;
    }

    ui.data_mut(|d| d.remove::<Draft>(id));
}

fn styled_edit(
    ui: &mut Ui,
    theme: &Theme,
    edit_id: Id,
    value: &mut String,
    placeholder: &str,
    purpose: TextPurpose,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), INPUT_H), Sense::hover());
    ui.painter().rect(
        rect,
        theme.rounding(theme.radius_card),
        theme.input_bg,
        Stroke::new(1.0, theme.window_edge),
        egui::StrokeKind::Inside,
    );
    let gate = focus::edit_gate(ui, rect, edit_id);

    let inner = rect.shrink2(vec2(10.0, 0.0));
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );

    row.spacing_mut().interact_size.y = INPUT_H;
    let mut edit = TextEdit::singleline(value)
        .id(edit_id)
        .interactive(gate.interactive)
        .desired_width(f32::INFINITY)
        .vertical_align(Align::Center)
        .margin(Margin::ZERO)
        .hint_text(placeholder)
        .frame(Frame::NONE);

    if purpose == TextPurpose::Secret {
        edit = edit.password(true);
    }

    let mut response = row.add(edit);
    let spec = TextInputSpec {
        purpose,
        action: TextAction::Done,
    };
    platform::text_input::field(&mut response, spec, value);
    focus::edit_done(ui, &response);
    response
}
