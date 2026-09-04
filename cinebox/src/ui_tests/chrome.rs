use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use rust_i18n::t;

use crate::nav::{NavAction, Screen};
use crate::theme::Theme;
use crate::widgets::chrome;
use crate::widgets::search::{self, SearchBar};

const HEADER_HARNESS: egui::Vec2 = vec2(800.0, 200.0);

struct HeaderState {
    screen: Screen,
    theme: Theme,
    action: Option<NavAction>,
    fonts: bool,
    settings_open: bool,
    search: SearchBar,
}

fn header_harness(screen: Screen) -> Harness<'static, HeaderState> {
    header_harness_with(screen, false, SearchBar::default())
}

fn header_harness_with(
    screen: Screen,
    settings_open: bool,
    search: SearchBar,
) -> Harness<'static, HeaderState> {
    let mut harness = Harness::builder()
        .with_size(HEADER_HARNESS)
        .build_ui_state(
            |ui, state| {
                if !state.fonts {
                    crate::fonts::install(ui.ctx());
                    egui_material_icons::initialize(ui.ctx());
                    state.theme.apply(ui.ctx());
                    state.fonts = true;
                    return;
                }

                if let Some(nav) = chrome::header(
                    ui,
                    state.screen,
                    &state.theme,
                    state.settings_open,
                    &mut state.search,
                ) {
                    state.action = Some(nav);
                }
            },
            HeaderState {
                screen,
                theme: Theme::dark(),
                action: None,
                fonts: false,
                settings_open,
                search,
            },
        );
    harness.run();
    harness
}

#[test]
fn home_header_settings_click_opens_settings() {
    let mut harness = header_harness(Screen::Home);
    harness
        .get_by_role_and_label(Role::Button, t!("nav.settings").as_ref())
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
        .get_by_role_and_label(Role::Button, t!("nav.back").as_ref())
        .click();

    harness.run();
    assert_eq!(harness.state().action, Some(NavAction::GoBack));
}

#[test]
fn home_header_back_when_settings_open() {
    let mut harness = header_harness_with(Screen::Home, true, SearchBar::default());
    harness
        .get_by_role_and_label(Role::Button, t!("nav.back").as_ref())
        .click();

    harness.run();
    assert_eq!(harness.state().action, Some(NavAction::GoBack));
}

#[test]
fn category_header_back_click_goes_back() {
    let mut harness = header_harness(Screen::Category {
        id: cinebox_core::HomeRowId::NowPlaying,
    });

    harness
        .get_by_role_and_label(Role::Button, t!("nav.back").as_ref())
        .click();

    harness.run();
    assert_eq!(harness.state().action, Some(NavAction::GoBack));
}

#[test]
fn search_header_back_click_goes_back() {
    let mut harness = header_harness(Screen::Search);
    harness
        .get_by_role_and_label(Role::Button, t!("nav.back").as_ref())
        .click();

    harness.run();
    assert_eq!(harness.state().action, Some(NavAction::GoBack));
}

#[test]
fn header_search_enter_opens_search() {
    let mut harness = header_harness(Screen::Home);
    harness
        .get_by_role_and_label(Role::TextInput, t!("search.placeholder").as_ref())
        .click();

    harness.run();
    let placeholder = t!("search.placeholder");
    let field = harness.get_by_role_and_label(Role::TextInput, placeholder.as_ref());

    field.focus();
    field.type_text("dune");

    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();

    assert_eq!(
        harness.state().action,
        Some(NavAction::OpenSearch {
            query: String::from("dune"),
        })
    );
}

#[test]
fn header_history_click_opens_search() {
    let mut harness = header_harness_with(
        Screen::Home,
        false,
        SearchBar::with_history(vec![String::from("dune")]),
    );

    harness
        .get_by_role_and_label(Role::TextInput, t!("search.placeholder").as_ref())
        .click();

    harness.run();
    harness.get_by_role_and_label(Role::Button, "dune").click();

    harness.run();
    assert_eq!(
        harness.state().action,
        Some(NavAction::OpenSearch {
            query: String::from("dune"),
        })
    );
}

#[test]
fn header_search_click_outside_closes_history() {
    let mut harness = header_harness_with(
        Screen::Home,
        false,
        SearchBar::with_history(vec![String::from("dune")]),
    );

    harness
        .get_by_role_and_label(Role::TextInput, t!("search.placeholder").as_ref())
        .click();

    harness.run();
    harness.get_by_role_and_label(Role::Button, "dune");

    let away = egui::pos2(20.0, 120.0);
    harness.hover_at(away);
    harness.drag_at(away);
    harness.drop_at(away);
    harness.run();

    assert!(
        harness
            .query_by_role_and_label(Role::Button, "dune")
            .is_none()
    );
}

#[test]
fn header_search_is_window_centered() {
    let harness = header_harness(Screen::Home);
    let edit = harness
        .get_by_role_and_label(Role::TextInput, t!("search.placeholder").as_ref())
        .rect();

    let field_center = edit.right() - search::SEARCH_W / 2.0;
    let window_center = HEADER_HARNESS.x / 2.0;

    assert!(
        (field_center - window_center).abs() < 4.0,
        "search field center {field_center}, window {window_center}"
    );
}
