//! The TV profile: D-pad focus, the remote's Back, interface sounds, and text
//! fields typed on the on-screen keyboard.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cinebox_core::UiLanguage;
use egui::accesskit::Role;
use egui::{Area, Id, Key, Order, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use rust_i18n::t;

use crate::nav::{NavAction, RailEntry, Screen};
use crate::platform::{
    self, Device, DeviceEvent, Direction, FieldText, Host, Profile, SpeechEvent, SpeechRequest,
    TextAction, TextInputEvent, TextInputSpec, TextPurpose, UiSound,
};
use crate::screens::OnboardingScreen;
use crate::services::{Services, db_block_on};
use crate::theme::Theme;
use crate::widgets::button::{self, Opts};
use crate::widgets::search::{self, SearchBar};
use crate::widgets::{field, focus, rail, scroll};

/// Records what the app asks of the OS and replays queued OS events.
#[derive(Default)]
struct TvDevice {
    speech: bool,
    log: Mutex<DeviceLog>,
    events: Mutex<VecDeque<DeviceEvent>>,
}

#[derive(Default)]
struct DeviceLog {
    sounds: Vec<UiSound>,
    /// `Some` for each keyboard opening, `None` for each closing.
    keyboard: Vec<Option<TextInputSpec>>,
    /// What the keyboard was told the field holds, in order.
    fields: Vec<FieldText>,
    speech_requests: usize,
    speech_stops: usize,
}

impl Device for TvDevice {
    fn play_sound(&self, sound: UiSound) {
        self.record(|log| log.sounds.push(sound));
    }

    fn speech_input_available(&self) -> bool {
        self.speech
    }

    fn start_speech_input(&self, _request: &SpeechRequest) {
        self.record(|log| log.speech_requests += 1);
    }

    fn stop_speech_input(&self) {
        self.record(|log| log.speech_stops += 1);
    }

    fn soft_keyboard(&self) -> bool {
        true
    }

    fn start_text_input(&self, spec: TextInputSpec, field: &FieldText) {
        self.record(|log| {
            log.keyboard.push(Some(spec));
            log.fields.push(field.clone());
        });
    }

    fn update_text_input(&self, field: &FieldText) {
        self.record(|log| log.fields.push(field.clone()));
    }

    fn stop_text_input(&self) {
        self.record(|log| log.keyboard.push(None));
    }

    fn poll_event(&self) -> Option<DeviceEvent> {
        self.events.lock().ok()?.pop_front()
    }
}

impl TvDevice {
    fn with_speech() -> Self {
        Self {
            speech: true,
            ..Self::default()
        }
    }

    fn record(&self, f: impl FnOnce(&mut DeviceLog)) {
        if let Ok(mut log) = self.log.lock() {
            f(&mut log);
        }
    }

    fn sounds(&self) -> Vec<UiSound> {
        self.log.lock().map(|log| log.sounds.clone()).unwrap_or_default()
    }

    fn keyboard(&self) -> Vec<Option<TextInputSpec>> {
        self.log.lock().map(|log| log.keyboard.clone()).unwrap_or_default()
    }

    fn fields(&self) -> Vec<FieldText> {
        self.log.lock().map(|log| log.fields.clone()).unwrap_or_default()
    }

    fn speech_requests(&self) -> usize {
        self.log.lock().map(|log| log.speech_requests).unwrap_or(0)
    }

    fn speech_stops(&self) -> usize {
        self.log.lock().map(|log| log.speech_stops).unwrap_or(0)
    }

    fn type_on_keyboard(&self, event: TextInputEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push_back(DeviceEvent::Text(event));
        }
    }
}

trait OnTv {
    fn device(&self) -> Arc<TvDevice>;
}

fn install(ui: &egui::Ui, device: Arc<TvDevice>) {
    let host = Host {
        profile: Profile::tv(),
        device,
        player: None,
    };
    platform::install(ui.ctx(), host);
}

/// One frame through the same hooks the app runs around its UI.
fn frame<S>(harness: &mut Harness<'_, S>) {
    let ctx = harness.ctx.clone();
    let input = harness.input_mut();
    let _for_app = platform::take_input(&ctx, input);
    focus::begin_frame(&ctx, input);
    harness.step();
}

fn end_frame(ui: &egui::Ui, theme: &Theme) {
    focus::end_frame(ui.ctx(), theme);
    platform::end_frame(ui.ctx());
}

/// One remote key press, then a few frames for the focus to settle.
fn press<S>(harness: &mut Harness<'_, S>, key: Key) {
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
    }

    // Like the app's repaints: a popup takes the focus a frame after it opens.
    for _ in 0..3 {
        frame(harness);
    }
}

fn settle<S>(harness: &mut Harness<'_, S>) {
    for _ in 0..3 {
        frame(harness);
    }
}

fn focused<S>(harness: &Harness<'_, S>, label: &str) -> bool {
    harness.get_by_label(label).is_focused()
}

struct TvState {
    theme: Option<Theme>,
    device: Arc<TvDevice>,
    clicked: Vec<&'static str>,
    popup: bool,
    value: String,
    search: SearchBar,
    searched: Option<NavAction>,
}

impl OnTv for TvState {
    fn device(&self) -> Arc<TvDevice> {
        Arc::clone(&self.device)
    }
}

fn harness(add: fn(&mut egui::Ui, &mut TvState)) -> Harness<'static, TvState> {
    harness_on(TvDevice::default(), add)
}

fn harness_on(device: TvDevice, add: fn(&mut egui::Ui, &mut TvState)) -> Harness<'static, TvState> {
    let state = TvState {
        theme: None,
        device: Arc::new(device),
        clicked: Vec::new(),
        popup: false,
        value: String::new(),
        search: SearchBar::default(),
        searched: None,
    };

    let mut harness = Harness::builder()
        .with_size(vec2(640.0, 360.0))
        .build_ui_state(
            move |ui, state| {
                install(ui, state.device());
                let Some(theme) = state.theme.clone() else {
                    crate::fonts::install(ui.ctx());
                    egui_material_icons::initialize(ui.ctx());
                    let theme = Theme::dark();
                    theme.apply(ui.ctx());
                    state.theme = Some(theme);
                    return;
                };

                add(ui, state);
                end_frame(ui, &theme);
            },
            state,
        );
    harness.run();
    harness
}

fn row_of_buttons(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    ui.horizontal(|ui| {
        for label in ["One", "Two", "Three"] {
            if button::label(ui, &theme, label, Opts::secondary(vec2(80.0, 32.0))) {
                state.clicked.push(label);
            }
        }
    });
}

#[test]
fn first_press_only_places_focus() {
    let mut harness = harness(row_of_buttons);

    press(&mut harness, Key::ArrowRight);
    assert!(focused(&harness, "One"), "focus should start on the first widget");

    press(&mut harness, Key::ArrowRight);
    assert!(focused(&harness, "Two"));
    assert!(harness.state().clicked.is_empty());
}

#[test]
fn ok_clicks_the_focused_widget() {
    let mut harness = harness(row_of_buttons);

    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::Enter);

    assert_eq!(harness.state().clicked, vec!["Two"]);
    assert_eq!(harness.state().device.sounds().last(), Some(&UiSound::Activate));
}

#[test]
fn moving_focus_sounds_and_the_edge_stays_quiet() {
    let mut harness = harness(row_of_buttons);

    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowRight);

    let step = UiSound::Navigate {
        direction: Direction::Right,
        repeat: false,
    };
    assert!(focused(&harness, "Three"));
    assert_eq!(harness.state().device.sounds(), vec![step, step]);
}

#[test]
fn starting_focus_prefers_the_screen_body() {
    let mut harness = harness(|ui, state| {
        let Some(theme) = state.theme.clone() else {
            return;
        };

        let _ = button::label(ui, &theme, "Header", Opts::secondary(vec2(80.0, 32.0)));
        let body = ui
            .scope(|ui| {
                let _ = button::label(ui, &theme, "Body", Opts::secondary(vec2(80.0, 32.0)));
            })
            .response
            .rect;
        focus::set_content(ui.ctx(), body);
    });

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Body"));
}

fn field_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    let id = Id::new("tv-field");
    let purpose = TextPurpose::Url;
    if let Some(text) = field::committed_edit(ui, &theme, id, &state.value, "", purpose) {
        state.value = text;
    }
    let _ = button::label(ui, &theme, "Below", Opts::secondary(vec2(80.0, 32.0)));
}

#[test]
fn text_field_waits_for_ok_before_typing() {
    let mut harness = harness(field_ui);

    press(&mut harness, Key::ArrowDown);
    assert!(!harness.ctx.text_edit_focused(), "the D-pad must not land inside the edit");

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Below"), "arrows pass over a field that is not being edited");

    press(&mut harness, Key::ArrowUp);
    press(&mut harness, Key::Enter);
    assert!(harness.ctx.text_edit_focused(), "OK starts typing");

    harness.get_by_role(Role::TextInput).type_text("tmdb");
    frame(&mut harness);
    press(&mut harness, Key::BrowserBack);

    assert!(!harness.ctx.text_edit_focused(), "Back stops typing");
    assert_eq!(harness.state().value, "tmdb");

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Below"), "the D-pad continues from the field");
}

#[test]
fn ok_on_a_field_opens_the_keyboard_and_its_confirm_commits() {
    let mut harness = harness(field_ui);
    let device = harness.state().device();

    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::Enter);
    let url = TextInputSpec {
        purpose: TextPurpose::Url,
        action: TextAction::Done,
    };
    assert_eq!(device.keyboard(), vec![Some(url)]);

    device.type_on_keyboard(edited("при", 3));
    device.type_on_keyboard(edited("привет", 6));
    device.type_on_keyboard(TextInputEvent::Action);
    settle(&mut harness);

    assert_eq!(harness.state().value, "привет");
    assert!(!harness.ctx.text_edit_focused());
    assert_eq!(device.keyboard(), vec![Some(url), None]);
}

#[test]
fn closing_the_keyboard_leaves_the_field() {
    let mut harness = harness(field_ui);
    let device = harness.state().device();

    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::Enter);
    device.type_on_keyboard(edited("tmdb", 4));
    device.type_on_keyboard(TextInputEvent::KeyboardHidden);
    settle(&mut harness);

    assert!(!harness.ctx.text_edit_focused());
    assert_eq!(harness.state().value, "tmdb");

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Below"), "the D-pad continues from the field");
}

#[test]
fn the_keyboard_reads_the_field_and_may_replace_all_of_it() {
    let mut harness = harness(field_ui);
    harness.state_mut().value = String::from("dune");
    let device = harness.state().device();

    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::Enter);
    assert_eq!(device.fields(), vec![field_text("dune", 4)], "typing starts from the field");

    device.type_on_keyboard(edited("alien", 2));
    settle(&mut harness);
    harness.get_by_role(Role::TextInput).type_text("x");
    settle(&mut harness);
    let typed_on_tv = field_text("alxien", 3);
    assert_eq!(device.fields().last(), Some(&typed_on_tv), "the keyboard follows the field");

    press(&mut harness, Key::BrowserBack);
    assert_eq!(harness.state().value, "alxien");
}

fn field_text(text: &str, cursor: usize) -> FieldText {
    FieldText {
        text: text.to_owned(),
        selection: cursor..cursor,
    }
}

fn edited(text: &str, cursor: usize) -> TextInputEvent {
    TextInputEvent::Edited(field_text(text, cursor))
}

/// A side menu on the current screen ("Movies") and the screen body beside
/// its last item.
fn side_menu_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    ui.horizontal(|ui| {
        let mut members = Vec::new();
        ui.vertical(|ui| {
            for label in ["Home", "Movies", "Shows", "Lists"] {
                let item = button::add(ui, &theme, label, Opts::secondary(vec2(100.0, 32.0)));
                members.push(item.id);
            }
        });
        let movies = members[1];
        focus::group(ui.ctx(), members, movies);

        ui.add_space(120.0);
        let body = ui
            .vertical(|ui| {
                ui.add_space(110.0);
                let _ = button::label(ui, &theme, "Body", Opts::secondary(vec2(100.0, 32.0)));
            })
            .response
            .rect;
        focus::set_content(ui.ctx(), body);
    });
}

#[test]
fn entering_a_side_menu_lands_on_its_current_item() {
    let mut harness = harness(side_menu_ui);

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Body"));

    press(&mut harness, Key::ArrowLeft);
    assert!(focused(&harness, "Movies"), "not the nearest item, the current one");

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Shows"), "inside the menu the arrows move as usual");
}

fn popup_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    ui.horizontal(|ui| {
        if button::label(ui, &theme, "Open", Opts::secondary(vec2(80.0, 32.0))) {
            state.popup = true;
        }
        let _ = button::label(ui, &theme, "Behind", Opts::secondary(vec2(80.0, 32.0)));
    });

    if !state.popup {
        return;
    }

    Area::new(Id::new("tv-popup"))
        .order(Order::Foreground)
        .fixed_pos(pos2(40.0, 120.0))
        .show(ui.ctx(), |ui| {
            focus::trap(ui);
            ui.horizontal(|ui| {
                let _ = button::label(ui, &theme, "Inside", Opts::secondary(vec2(80.0, 32.0)));
                let _ = button::label(ui, &theme, "Also", Opts::secondary(vec2(80.0, 32.0)));
            });
        });

    if ui.input(|i| i.key_pressed(Key::Escape)) {
        state.popup = false;
    }
}

#[test]
fn popup_keeps_the_dpad_and_gives_it_back() {
    let mut harness = harness(popup_ui);

    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::Enter);
    assert!(harness.state().popup);
    assert!(focused(&harness, "Inside"), "focus moves into the popup");

    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowUp);
    assert!(focused(&harness, "Also"), "the D-pad cannot reach widgets behind the popup");

    press(&mut harness, Key::BrowserBack);
    frame(&mut harness);
    assert!(!harness.state().popup);
    assert!(focused(&harness, "Open"), "focus returns to what opened the popup");
}

fn search_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    let bar = Rect::from_min_size(pos2(40.0, 40.0), vec2(380.0, 28.0));
    if let Some(action) = state.search.show(ui, &theme, bar) {
        state.searched = Some(action);
    }
}

#[test]
fn search_bar_is_one_stop_with_the_microphone_beside_it() {
    let mut harness = harness_on(TvDevice::with_speech(), search_ui);
    let device = harness.state().device();

    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::Enter);

    assert_eq!(device.speech_requests(), 1);
}

#[test]
fn search_bar_has_no_microphone_without_speech_input() {
    let harness = harness(search_ui);

    let voice = t!("search.voice");
    assert!(harness.query_by_label(voice.as_ref()).is_none());
}

#[test]
fn keyboard_search_key_submits_the_query() {
    let mut harness = harness(search_ui);
    let device = harness.state().device();

    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::Enter);
    let search = TextInputSpec {
        purpose: TextPurpose::Text,
        action: TextAction::Search,
    };
    assert_eq!(device.keyboard(), vec![Some(search)]);

    device.type_on_keyboard(edited("дюна", 4));
    device.type_on_keyboard(TextInputEvent::Action);
    settle(&mut harness);

    let query = String::from("дюна");
    assert_eq!(harness.state().searched, Some(NavAction::OpenSearch { query }));
}

#[test]
fn speech_fills_the_field_and_searches_the_final_result() {
    let mut bar = SearchBar::default();

    bar.on_speech(SpeechEvent::Listening);
    bar.on_speech(SpeechEvent::Partial(String::from("дю")));
    assert_eq!(bar.query, "дю");

    let action = bar.on_speech(SpeechEvent::Final(String::from("  дюна ")));
    let query = String::from("дюна");
    assert_eq!(action, Some(NavAction::OpenSearch { query }));
    assert_eq!(bar.query, "дюна");
}

#[test]
fn the_microphone_again_or_back_stops_listening() {
    let mut harness = harness_on(TvDevice::with_speech(), search_ui);
    let device = harness.state().device();

    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::Enter);
    assert_eq!(device.speech_stops(), 1, "OK on the microphone again stops");

    press(&mut harness, Key::Enter);
    let ctx = harness.ctx.clone();
    let consumed = harness.state_mut().search.consume_escape(&ctx);
    assert!(consumed, "Back stops listening before anything else");
    assert_eq!(device.speech_stops(), 2);
}

const ROWS: [&str; 16] = [
    "Row 1", "Row 2", "Row 3", "Row 4", "Row 5", "Row 6", "Row 7", "Row 8", "Row 9", "Row 10",
    "Row 11", "Row 12", "Row 13", "Row 14", "Row 15", "Row 16",
];

const SHELVES: [[&str; 8]; 4] = [
    ["A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8"],
    ["B1", "B2", "B3", "B4", "B5", "B6", "B7", "B8"],
    ["C1", "C2", "C3", "C4", "C5", "C6", "C7", "C8"],
    ["D1", "D2", "D3", "D4", "D5", "D6", "D7", "D8"],
];

fn long_list_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    scroll::vertical(ui, "tv-list", |ui| {
        for label in ROWS {
            let _ = button::label(ui, &theme, label, Opts::secondary(vec2(160.0, 40.0)));
        }
    });
}

fn shelves_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    let poster = Opts::secondary(vec2(140.0, 120.0));
    scroll::vertical(ui, "tv-page", |ui| {
        for (row, shelf) in SHELVES.iter().enumerate() {
            scroll::horizontal(ui, ("tv-shelf", row), |ui| {
                ui.horizontal(|ui| {
                    for label in shelf {
                        let _ = button::label(ui, &theme, label, poster);
                    }
                });
            });
        }
    });
}

/// A side menu beside one shelf, like Home: the shelf starts right of the menu.
fn menu_and_shelf_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    let item = Opts::secondary(vec2(48.0, 48.0));
    let poster = Opts::secondary(vec2(140.0, 210.0));
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.add_space(60.0);
            for label in ["Home", "Movies", "Shows"] {
                let _ = button::label(ui, &theme, label, item);
            }
        });

        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.add_space(40.0);
            scroll::horizontal(ui, "tv-menu-shelf", |ui| {
                ui.horizontal(|ui| {
                    for label in SHELVES[0] {
                        let _ = button::label(ui, &theme, label, poster);
                    }
                });
            });
        });
    });
}

const TILES: [&str; 5] = ["T1", "T2", "T3", "T4", "T5"];

/// A search bar above a scrolling page of tall tiles, like the header over Home.
fn search_over_page_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    let _ = button::label(ui, &theme, "Search", Opts::secondary(vec2(400.0, 32.0)));
    scroll::vertical(ui, "tv-tiles", |ui| {
        for label in TILES {
            let _ = button::label(ui, &theme, label, Opts::secondary(vec2(300.0, 200.0)));
        }
    });
}

/// The app's header over a page: the search bar centred, settings at the right
/// edge, and a row of tiles across the page below.
fn header_over_page_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    let bar = Rect::from_min_size(pos2(170.0, 8.0), vec2(300.0, 28.0));
    let _ = state.search.show(ui, &theme, bar);

    let settings_rect = Rect::from_min_size(pos2(596.0, 6.0), vec2(32.0, 32.0));
    let settings = ui.put(settings_rect, |ui: &mut egui::Ui| {
        button::add(ui, &theme, "Settings", Opts::secondary(vec2(32.0, 32.0)))
    });

    let [field, mic] = search::stop_ids();
    focus::group(ui.ctx(), vec![field, mic, settings.id], field);

    for (label, x) in [("Left", 20.0), ("Middle", 260.0), ("Right", 500.0)] {
        let tile = Rect::from_min_size(pos2(x, 140.0), vec2(120.0, 80.0));
        ui.put(tile, |ui: &mut egui::Ui| {
            button::add(ui, &theme, label, Opts::secondary(vec2(120.0, 80.0)))
        });
    }

    let page = Rect::from_min_max(pos2(0.0, 100.0), pos2(640.0, 360.0));
    focus::set_content(ui.ctx(), page);
}

fn search_focused<S>(harness: &Harness<'_, S>) -> bool {
    let [field, _] = search::stop_ids();
    harness.ctx.memory(|mem| mem.has_focus(field))
}

#[test]
fn up_from_far_left_of_the_page_reaches_the_search_bar() {
    let mut harness = harness(header_over_page_ui);

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Left"));

    press(&mut harness, Key::ArrowUp);
    assert!(search_focused(&harness), "nothing straight above, yet Up still reaches the bar");
}

#[test]
fn up_under_settings_lands_on_the_search_bar() {
    let mut harness = harness(header_over_page_ui);

    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowRight);
    assert!(focused(&harness, "Right"));

    press(&mut harness, Key::ArrowUp);
    assert!(search_focused(&harness), "the bar is entered at its field");

    press(&mut harness, Key::ArrowRight);
    assert!(focused(&harness, "Settings"), "inside the bar the arrows move as usual");
}

/// A details page: a heading with no stop of its own, the main action under
/// it, and a long list further down, like Media.
fn details_page_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    scroll::vertical(ui, "tv-details", |ui| {
        ui.add_sized(vec2(600.0, 120.0), egui::Label::new("Heading"));
        let _ = button::label(ui, &theme, "Watch", Opts::secondary(vec2(160.0, 40.0)));
        for label in ROWS {
            let _ = button::label(ui, &theme, label, Opts::secondary(vec2(160.0, 40.0)));
        }
    });
}

#[test]
fn back_up_to_the_first_row_shows_the_heading_again() {
    let mut harness = harness(details_page_ui);

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Watch"));
    for _ in ROWS {
        press(&mut harness, Key::ArrowDown);
    }
    assert!(!on_screen(&harness, "Heading"));

    for _ in ROWS {
        press(&mut harness, Key::ArrowUp);
    }
    settle(&mut harness);

    assert!(focused(&harness, "Watch"));
    assert!(on_screen(&harness, "Heading"), "the page is back at its top");
}

/// The side menu on Movies beside a page with one tile.
fn rail_and_page_ui(ui: &mut egui::Ui, state: &mut TvState) {
    let Some(theme) = state.theme.clone() else {
        return;
    };

    let body = Rect::from_min_max(pos2(0.0, 40.0), pos2(640.0, 360.0));
    let movies = RailEntry::Section(cinebox_core::Section::Movies);
    let _ = rail::show(ui, body, &theme, Some(movies));

    let tile = Rect::from_min_size(pos2(260.0, 120.0), vec2(120.0, 80.0));
    ui.put(tile, |ui: &mut egui::Ui| {
        button::add(ui, &theme, "Tile", Opts::secondary(vec2(120.0, 80.0)))
    });

    let page = Rect::from_min_max(pos2(200.0, 40.0), pos2(640.0, 360.0));
    focus::set_content(ui.ctx(), page);
}

#[test]
fn back_from_a_menu_page_goes_to_the_menu_before_leaving() {
    let mut harness = harness(rail_and_page_ui);
    let movies = Screen::Section {
        section: cinebox_core::Section::Movies,
    };

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Tile"));

    let ctx = harness.ctx.clone();
    assert!(crate::app::back_to_rail(&ctx, movies), "the first Back stays in the app");
    settle(&mut harness);
    assert!(rail::focused(&ctx));

    assert!(!crate::app::back_to_rail(&ctx, movies), "from the menu Back goes on");
}

fn on_screen<S>(harness: &Harness<'_, S>, label: &str) -> bool {
    let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(640.0, 360.0));

    screen.contains_rect(harness.get_by_label(label).rect())
}

#[test]
fn a_long_list_scrolls_to_the_focused_row() {
    let mut harness = harness(long_list_ui);

    for _ in ROWS {
        press(&mut harness, Key::ArrowDown);
    }
    settle(&mut harness);

    assert!(focused(&harness, "Row 16"));
    assert!(on_screen(&harness, "Row 16"));
}

#[test]
fn a_shelf_and_the_page_around_it_both_scroll_to_the_focus() {
    let mut harness = harness(shelves_ui);

    for _ in SHELVES {
        press(&mut harness, Key::ArrowDown);
    }
    for _ in 1..SHELVES[3].len() {
        press(&mut harness, Key::ArrowRight);
    }
    settle(&mut harness);

    assert!(focused(&harness, "D8"));
    assert!(on_screen(&harness, "D8"));
}

#[test]
fn left_along_a_scrolled_shelf_stays_in_the_shelf() {
    let mut harness = harness(menu_and_shelf_ui);

    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowRight);
    assert!(focused(&harness, "A1"));
    for _ in 1..SHELVES[0].len() {
        press(&mut harness, Key::ArrowRight);
    }
    assert!(focused(&harness, "A8"));

    for label in SHELVES[0].iter().rev().skip(1) {
        press(&mut harness, Key::ArrowLeft);
        assert!(focused(&harness, label), "Left should land on {label}");
    }

    press(&mut harness, Key::ArrowLeft);
    assert!(focused(&harness, "Movies"), "past the first item the menu takes over");
}

#[test]
fn up_a_scrolled_page_stays_in_the_page() {
    let mut harness = harness(search_over_page_ui);

    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "T1"));
    for _ in 1..TILES.len() {
        press(&mut harness, Key::ArrowDown);
    }
    assert!(focused(&harness, "T5"));

    for label in TILES.iter().rev().skip(1) {
        press(&mut harness, Key::ArrowUp);
        assert!(focused(&harness, label), "Up should land on {label}");
    }

    press(&mut harness, Key::ArrowUp);
    assert!(focused(&harness, "Search"), "past the first tile the search takes over");
}

struct WizardTv {
    wizard: OnboardingScreen,
    svc: Services,
    theme: Theme,
    fonts: bool,
    device: Arc<TvDevice>,
}

impl OnTv for WizardTv {
    fn device(&self) -> Arc<TvDevice> {
        Arc::clone(&self.device)
    }
}

fn wizard_harness(language: UiLanguage) -> Harness<'static, WizardTv> {
    let Ok(store) = db_block_on(cinebox_core::Store::memory()) else {
        panic!("in-memory store");
    };

    let mut wizard = OnboardingScreen::default();
    wizard.open_offline();
    let mut svc = Services::test_with_db(Arc::new(store));
    svc.settings.general.language = language;

    let state = WizardTv {
        wizard,
        svc,
        theme: Theme::dark(),
        fonts: false,
        device: Arc::new(TvDevice::default()),
    };

    let mut harness = Harness::builder()
        .with_size(vec2(1000.0, 800.0))
        .build_ui_state(draw_wizard_over_screen, state);
    settle(&mut harness);

    harness
}

/// The wizard over a screen full of focusable rows, like Home behind it.
fn draw_wizard_over_screen(ui: &mut egui::Ui, state: &mut WizardTv) {
    install(ui, state.device());
    if !state.fonts {
        crate::fonts::install(ui.ctx());
        egui_material_icons::initialize(ui.ctx());
        state.theme.apply(ui.ctx());
        state.fonts = true;
        return;
    }

    ui.vertical_centered(|ui| {
        for row in 0..24 {
            let label = format!("Behind {row}");
            let opts = Opts::secondary(vec2(400.0, 20.0));
            let _ = button::label(ui, &state.theme, &label, opts);
        }
    });

    let ctx = ui.ctx().clone();
    platform::set_overscan(&ctx, state.svc.settings.general.overscan);
    state.wizard.ui(&ctx, &mut state.svc, &state.theme);
    end_frame(ui, &state.theme);
}

#[test]
fn wizard_starts_on_the_current_language() {
    let harness = wizard_harness(UiLanguage::Russian);

    assert!(focused(&harness, "Русский"));
}

#[test]
fn wizard_dpad_walks_the_card_not_the_screen_behind() {
    let mut harness = wizard_harness(UiLanguage::Russian);

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, "Українська"));

    press(&mut harness, Key::ArrowDown);
    assert!(focused(&harness, &t!("wizard.next")));

    press(&mut harness, Key::ArrowUp);
    press(&mut harness, Key::ArrowUp);
    press(&mut harness, Key::ArrowUp);
    assert!(focused(&harness, "English"));
}

#[test]
fn wizard_fits_the_screen_with_left_and_right() {
    let mut harness = wizard_harness(UiLanguage::English);
    let inset = |harness: &Harness<'_, WizardTv>| platform::edge_inset(&harness.ctx);
    let before = inset(&harness);

    for _ in 0..3 {
        press(&mut harness, Key::ArrowDown);
    }
    press(&mut harness, Key::Enter);
    assert!(focused(&harness, "Edge margin 2%"), "the step opens on its control");

    press(&mut harness, Key::ArrowRight);
    assert!(focused(&harness, "Edge margin 2.5%"), "the arrows change the value, not the focus");
    assert!(inset(&harness).left > before.left, "the interface moves in at once");

    press(&mut harness, Key::ArrowLeft);
    press(&mut harness, Key::ArrowLeft);
    assert_eq!(harness.state().svc.settings.general.overscan.permille(), 15);
}
