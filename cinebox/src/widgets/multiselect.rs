//! ComboBox that keeps several values checked.

use egui::{ComboBox, PopupCloseBehavior, RichText, Ui};
use rust_i18n::t;

use super::button;
use super::chips;
use super::combo;
use crate::theme::Theme;
use crate::widgets::focus;

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
        let combo = ComboBox::from_id_salt(id)
            .width(width)
            .selected_text(selected_text)
            .popup_style(combo::popup_style(theme))
            // Several values: picking one keeps the list open for the next.
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                focus::trap(ui);
                let opening = focus::focused_layer(ui.ctx()) != Some(ui.layer_id());
                let first_on = options.iter().position(|opt| selected.contains(opt));
                let entry = first_on.unwrap_or(0);
                for (index, opt) in options.iter().enumerate() {
                    let on = selected.contains(opt);
                    let row = ui.selectable_label(on, label(*opt));
                    focus::track(&row);
                    if opening && index == entry {
                        focus::enter_popup(&row);
                    }

                    if !row.clicked() {
                        continue;
                    }

                    chips::toggle(selected, *opt);
                    changed = true;
                }
            });
        button::pointing(combo.response);
    });

    changed
}

fn closed_label<T: Copy>(selected: &[T], label: &impl Fn(T) -> String) -> String {
    if selected.is_empty() {
        return t!("filter.any").into_owned();
    }

    if let [only] = selected {
        return label(*only);
    }

    format!("{} {}", selected.len(), t!("filter.selected"))
}
