use cinebox_core::i18n::Msg;
use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::nav::{NavAction, Screen};
use crate::theme::Theme;
use crate::widgets::chrome;

struct HeaderState {
    screen: Screen,
    theme: Theme,
    action: Option<NavAction>,
    fonts: bool,
    settings_open: bool,
}

fn header_harness(screen: Screen) -> Harness<'static, HeaderState> {
    header_harness_with(screen, false)
}

fn header_harness_with(screen: Screen, settings_open: bool) -> Harness<'static, HeaderState> {
    let mut harness = Harness::builder()
        .with_size(vec2(800.0, 80.0))
        .build_ui_state(
            |ui, state| {
                if !state.fonts {
                    crate::fonts::install(ui.ctx());
                    egui_material_icons::initialize(ui.ctx());
                    state.theme.apply(ui.ctx());
                    state.fonts = true;
                    return;
                }
                if let Some(nav) =
                    chrome::header(ui, state.screen, &state.theme, state.settings_open)
                {
                    state.action = Some(nav);
                }
            },
            HeaderState {
                screen,
                theme: Theme::dark(),
                action: None,
                fonts: false,
                settings_open,
            },
        );
    harness.run();
    harness
}

#[test]
fn home_header_settings_click_opens_settings() {
    let mut harness = header_harness(Screen::Home);
    harness
        .get_by_role_and_label(Role::Button, Msg::NavSettings.en())
        .click();
    
    harness.run();
    assert_eq!(harness.state().action, Some(NavAction::OpenSettings));
}

#[test]
fn media_header_back_click_goes_back() {
    let mut harness = header_harness(Screen::Media {
        kind: cinebox_core::MediaKind::Movie,
        id: cinebox_core::TmdbId::new(1),
    });

    harness
        .get_by_role_and_label(Role::Button, Msg::NavBack.en())
        .click();

    harness.run();
    assert_eq!(harness.state().action, Some(NavAction::GoBack));
}

#[test]
fn home_header_back_when_settings_open() {
    let mut harness = header_harness_with(Screen::Home, true);
    harness
        .get_by_role_and_label(Role::Button, Msg::NavBack.en())
        .click();

    harness.run();
    assert_eq!(harness.state().action, Some(NavAction::GoBack));
}
