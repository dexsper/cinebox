//! Multi-toggle chip rows shared by settings and torrent filters.

use cinebox_core::i18n::Msg;
use egui::Ui;

use super::button::{self, Opts};
use crate::theme::Theme;

pub fn toggle<T: Copy + PartialEq>(selected: &mut Vec<T>, value: T) {
    if let Some(index) = selected.iter().position(|item| *item == value) {
        selected.remove(index);
        return;
    }

    selected.push(value);
}

pub fn multi_row<T: Copy + PartialEq>(
    ui: &mut Ui,
    theme: &Theme,
    selected: &mut Vec<T>,
    options: &[T],
    label: impl Fn(T) -> String,
) -> bool {
    let mut changed = false;

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let any_on = selected.is_empty();
        let any_clicked = chip(ui, theme, Msg::FilterAny.t(), any_on);
        
        if any_clicked && !any_on {
            selected.clear();
            changed = true;
        }

        for opt in options {
            let active = selected.contains(opt);
            if !chip(ui, theme, &label(*opt), active) {
                continue;
            }

            toggle(selected, *opt);
            changed = true;
        }
    });

    changed
}

fn chip(ui: &mut Ui, theme: &Theme, label: &str, active: bool) -> bool {
    button::label(ui, theme, label, Opts::chip(active))
}
