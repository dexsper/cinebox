use cinebox_core::{LibraryMark, ListStatus, Section};
use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use egui_material_icons::icons::ICON_LOCAL_MOVIES;
use rust_i18n::t;

use crate::nav::RailEntry;
use crate::theme::Theme;
use crate::widgets::button::{self, ExpandingIcon};
use crate::widgets::lists::{self, MenuPick};
use crate::widgets::rail;

struct State<T> {
    theme: Theme,
    fonts: bool,
    out: Option<T>,
}

impl<T> State<T> {
    fn new() -> Self {
        Self {
            theme: Theme::dark(),
            fonts: false,
            out: None,
        }
    }
}

/// First frame installs fonts and icons; later frames run `paint`.
fn harness<T: 'static>(
    mut paint: impl FnMut(&mut egui::Ui, &Theme) -> Option<T> + 'static,
) -> Harness<'static, State<T>> {
    Harness::builder()
        .with_size(vec2(640.0, 480.0))
        .build_ui_state(
            move |ui, state: &mut State<T>| {
                if !state.fonts {
                    crate::fonts::install(ui.ctx());
                    egui_material_icons::initialize(ui.ctx());
                    state.theme.apply(ui.ctx());
                    state.fonts = true;
                    return;
                }

                if let Some(out) = paint(ui, &state.theme) {
                    state.out = Some(out);
                }
            },
            State::new(),
        )
}

#[test]
fn collapsed_rail_item_is_clickable_by_its_label() {
    let mut harness = harness(|ui, theme| {
        let body = ui.max_rect();
        rail::show(ui, body, theme, Some(RailEntry::Home))
    });
    harness.run();

    harness
        .get_by_role_and_label(Role::Button, t!("rail.anime").as_ref())
        .click();
    harness.run();

    assert_eq!(harness.state().out, Some(RailEntry::Section(Section::Anime)));
}

#[test]
fn collapsed_icon_button_is_clickable_by_its_label() {
    let mut harness = harness(|ui, theme| {
        let label = t!("media.trailers");
        let spec = ExpandingIcon {
            id_salt: "test-trailers",
            icon: ICON_LOCAL_MOVIES,
            label: label.as_ref(),
            height: 46.0,
            tint: theme.title,
            selected: false,
        };
        button::expanding_icon(ui, theme, spec).clicked().then_some(())
    });
    harness.run();

    harness
        .get_by_role_and_label(Role::Button, t!("media.trailers").as_ref())
        .click();
    harness.run();

    assert_eq!(harness.state().out, Some(()));
}

mod lists_menu {
    use super::*;

    fn menu_harness(mark: LibraryMark) -> Harness<'static, State<MenuPick>> {
        harness(move |ui, theme| lists::menu(ui, theme, mark))
    }

    #[test]
    fn picking_a_status_replaces_the_current_one() {
        let mut harness = menu_harness(LibraryMark {
            status: Some(ListStatus::Planned),
            liked: false,
        });
        harness.run();

        harness
            .get_by_role_and_label(Role::CheckBox, t!("library.completed").as_ref())
            .click();
        harness.run();

        assert_eq!(
            harness.state().out,
            Some(MenuPick::Status(Some(ListStatus::Completed)))
        );
    }

    #[test]
    fn picking_the_active_status_clears_it() {
        let mut harness = menu_harness(LibraryMark {
            status: Some(ListStatus::Watching),
            liked: true,
        });
        harness.run();

        harness
            .get_by_role_and_label(Role::CheckBox, t!("library.watching").as_ref())
            .click();
        harness.run();

        assert_eq!(harness.state().out, Some(MenuPick::Status(None)));
    }
}
