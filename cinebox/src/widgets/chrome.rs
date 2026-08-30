//! Top chrome: title + settings/back.

use cinebox_core::i18n::Msg;
use egui::{RichText, Ui};
use egui_material_icons::icons::{ICON_ARROW_BACK, ICON_SETTINGS};

use crate::nav::{NavAction, Screen};
use crate::theme::Theme;

fn bleed_screen(screen: Screen) -> bool {
    matches!(
        screen,
        Screen::Media { .. } | Screen::Person { .. } | Screen::Torrents { .. }
    )
}

pub fn header(ui: &mut Ui, screen: Screen, theme: &Theme) -> Option<NavAction> {
    if matches!(screen, Screen::Player { .. }) {
        return None;
    }
    let pad = if bleed_screen(screen) {
        theme.pad.round() as i8
    } else {
        0
    };
    let mut action = None;
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: pad,
            right: pad,
            top: pad,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(Msg::AppTitle.en()).size(22.0).color(theme.title));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match screen {
                        Screen::Home => {
                            if icon_btn(ui, ICON_SETTINGS, Msg::NavSettings.en()) {
                                action = Some(NavAction::OpenSettings);
                            }
                        }
                        Screen::Player { .. } => {}
                        _ => {
                            if icon_btn(ui, ICON_ARROW_BACK, Msg::NavBack.en()) {
                                action = Some(NavAction::GoBack);
                            }
                        }
                    }
                });
            });
        });
    action
}

fn icon_btn(ui: &mut Ui, icon: egui_material_icons::MaterialIcon, hint: &str) -> bool {
    ui.add(egui::Button::new(icon.rich_text().size(20.0)))
        .on_hover_text(hint)
        .clicked()
}
