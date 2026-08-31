use cinebox_core::i18n::Msg;
use cinebox_parse::SortMode;
use egui::accesskit::Role;
use egui::{Frame, RichText, TextEdit, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use egui_material_icons::icons::{ICON_FILTER_LIST, ICON_PLAY_CIRCLE};

use crate::theme::Theme;
use crate::widgets::button::{self, CHIP_MIN_W, Opts};
use crate::widgets::{self, combo};

struct InteractState {
    theme: Theme,
    fonts: bool,
    retry: bool,
    watch: bool,
    watch_hover: bool,
    filters: bool,
    filters_hover: bool,
    sort: SortMode,
    query: String,
    hit: bool,
    hit_hover: bool,
}

fn interact_harness() -> Harness<'static, InteractState> {
    let mut harness = Harness::builder()
        .with_size(vec2(640.0, 720.0))
        .build_ui_state(
            |ui, state| {
                if !state.fonts {
                    crate::fonts::install(ui.ctx());
                    egui_material_icons::initialize(ui.ctx());
                    state.theme.apply(ui.ctx());
                    state.fonts = true;
                    return;
                }

                let theme = &state.theme;
                ui.vertical(|ui| {
                    ui.set_width(400.0);
                    let watch = button::add_named(
                        ui,
                        theme,
                        (
                            egui::Atom::grow(),
                            ICON_PLAY_CIRCLE
                                .rich_text()
                                .size(theme.text_cta_icon)
                                .color(theme.btn_primary_fg),
                            RichText::new(Msg::WatchTorrents.en())
                                .font(theme.emphasis_font(theme.text_subtitle))
                                .color(theme.btn_primary_fg),
                            egui::Atom::grow(),
                        ),
                        Opts::primary(vec2(176.0, 46.0)),
                        Some(Msg::WatchTorrents.en()),
                    );
                    
                    state.watch_hover = watch.hovered();
                    if watch.clicked() {
                        state.watch = true;
                    }

                    let icon = ICON_FILTER_LIST;
                    let filters_pad = button::icon_label_pad_y(ui, theme, icon, combo::HEIGHT);

                    let filters = button::add_named(
                        ui,
                        theme,
                        (
                            icon.rich_text().size(theme.text_icon).color(theme.title),
                            RichText::new(Msg::Filters.en())
                                .size(theme.text_body)
                                .color(theme.title),
                        ),
                        Opts::secondary(vec2(118.0, combo::HEIGHT)).pad_y(filters_pad),
                        Some(Msg::Filters.en()),
                    );

                    state.filters_hover = filters.hovered();
                    if filters.clicked() {
                        state.filters = true;
                    }

                    combo::show_with(
                        ui,
                        theme,
                        "test-sort",
                        &mut state.sort,
                        SortMode::ALL,
                        |mode| sort_label(mode).to_owned(),
                    );

                    ui.add(TextEdit::singleline(&mut state.query).hint_text("search"));

                    let id = ui.id().with("hit-row");
                    let fill = button::fill_for_hover(ui, id, theme.card, theme.widget_hover);
                    let shown = Frame::new().fill(fill).inner_margin(12.0).show(ui, |ui| {
                        ui.label("The Hit");
                    });

                    let hit = button::click_rect(ui, id, shown.response.rect);
                    state.hit_hover = hit.hovered();
                    
                    if hit.clicked() {
                        state.hit = true;
                    }

                    ui.horizontal(|ui| {
                        button::label(ui, theme, Msg::No.en(), Opts::chip(false));
                        button::label(ui, theme, Msg::Yes.en(), Opts::chip(false));
                        button::label(ui, theme, Msg::FilterAny.en(), Opts::chip(true));
                    });
                });

                if widgets::page_error(ui, theme, Msg::NeedParser.en()) {
                    state.retry = true;
                }
            },
            InteractState {
                theme: Theme::dark(),
                fonts: false,
                retry: false,
                watch: false,
                watch_hover: false,
                filters: false,
                filters_hover: false,
                sort: SortMode::Popular,
                query: String::new(),
                hit: false,
                hit_hover: false,
            },
        );
    harness.run();
    harness
}

fn sort_label(mode: SortMode) -> &'static str {
    match mode {
        SortMode::Popular => Msg::SortPopular.en(),
        SortMode::Seeders => Msg::SortSeeders.en(),
        SortMode::Size => Msg::SortSize.en(),
    }
}

#[test]
fn watch_button_hover_and_click() {
    let mut harness = interact_harness();
    harness
        .get_by_role_and_label(Role::Button, Msg::WatchTorrents.en())
        .hover();
    harness.run();
    assert!(harness.state().watch_hover);

    harness
        .get_by_role_and_label(Role::Button, Msg::WatchTorrents.en())
        .click();
    harness.run();
    assert!(harness.state().watch);
}

#[test]
fn filters_hover_and_retry_click() {
    let mut harness = interact_harness();
    harness
        .get_by_role_and_label(Role::Button, Msg::Filters.en())
        .hover();
    harness.run();
    assert!(harness.state().filters_hover);

    harness
        .get_by_role_and_label(Role::Button, Msg::Retry.en())
        .click();
    harness.run();
    assert!(harness.state().retry);
}

#[test]
fn combo_selects_another_option() {
    let mut harness = interact_harness();
    harness.get_by_role(Role::ComboBox).click();
    harness.run();
    harness.get_by_label(Msg::SortSeeders.en()).click();
    harness.run();
    assert_eq!(harness.state().sort, SortMode::Seeders);
}

#[test]
fn torrent_row_hover_and_click() {
    let mut harness = interact_harness();
    harness.get_by_label("The Hit").hover();
    harness.run();
    assert!(harness.state().hit_hover);

    harness.get_by_label("The Hit").click();
    harness.run();
    assert!(harness.state().hit);
}

#[test]
fn filters_button_matches_combo_height() {
    let harness = interact_harness();
    let button_h = harness
        .get_by_role_and_label(Role::Button, Msg::Filters.en())
        .rect()
        .height();
    let combo_h = harness.get_by_role(Role::ComboBox).rect().height();

    assert!(
        (button_h - combo_h).abs() < 0.5,
        "filters button {button_h} vs combo {combo_h}"
    );
    assert!(
        (combo_h - combo::HEIGHT).abs() < 0.5,
        "combo {combo_h} should be {}",
        combo::HEIGHT
    );
}

#[test]
fn chips_share_min_width() {
    let harness = interact_harness();
    let no_w = harness
        .get_by_role_and_label(Role::Button, Msg::No.en())
        .rect()
        .width();
    let yes_w = harness
        .get_by_role_and_label(Role::Button, Msg::Yes.en())
        .rect()
        .width();
    let any_w = harness
        .get_by_role_and_label(Role::Button, Msg::FilterAny.en())
        .rect()
        .width();

    assert!(no_w >= CHIP_MIN_W);
    assert!(yes_w >= CHIP_MIN_W);
    assert!(any_w >= CHIP_MIN_W);
    assert!((no_w - yes_w).abs() < 1.0);
}
