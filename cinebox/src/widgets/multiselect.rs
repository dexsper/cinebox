//! ComboBox that keeps several values checked.

use cinebox_core::i18n::Msg;
use egui::{ComboBox, CursorIcon, RichText, Ui};

use super::chips;
use super::combo;
use crate::theme::Theme;

pub fn show_with<T: Copy + PartialEq>(
    ui: &mut Ui,
    theme: &Theme,
    id: &str,
    selected: &mut Vec<T>,
    options: &[T],
    label: impl Fn(T) -> String,
) -> bool {
    let mut changed = false;
    let width = ui.available_width();
    let text = closed_label(selected, &label);
    let selected_text = RichText::new(text).color(theme.label);

    ui.scope(|ui| {
        combo::apply_visuals(ui, theme);
        ComboBox::from_id_salt(id)
            .width(width)
            .selected_text(selected_text)
            .popup_style(combo::popup_style(theme))
            .show_ui(ui, |ui| {
                for opt in options {
                    let mut on = selected.contains(opt);
                    if !ui.checkbox(&mut on, label(*opt)).changed() {
                        continue;
                    }

                    chips::toggle(selected, *opt);
                    changed = true;
                }
            })
            .response
            .on_hover_cursor(CursorIcon::PointingHand);
    });

    changed
}

fn closed_label<T: Copy>(selected: &[T], label: &impl Fn(T) -> String) -> String {
    if selected.is_empty() {
        return Msg::FilterAny.t().to_owned();
    }

    if let [only] = selected {
        return label(*only);
    }

    format!("{} {}", selected.len(), Msg::Selected.t())
}
