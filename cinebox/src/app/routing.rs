//! What a [`NavAction`] does to the screen stack, and where Back leads.

use egui::{Context, Key, Ui};
use tracing::error;

use super::App;
use super::nav::{NavAction, RailEntry, Screen};
use crate::platform;
use crate::services::db_block_on;
use crate::widgets::{focus, rail};

impl App {
    pub(super) fn apply_nav(&mut self, action: NavAction, now: f64, ctx: &Context) {
        let before = self.nav.current();
        self.nav.mark_focus(ctx.memory(|mem| mem.focused()));
        self.apply_nav_action(action, now, ctx);

        let returned = self.nav.current() != before;
        if let Some(id) = self.nav.focus_mark().filter(|_| returned) {
            ctx.memory_mut(|mem| mem.request_focus(id));
        }
    }

    fn apply_nav_action(&mut self, action: NavAction, now: f64, ctx: &Context) {
        match action {
            NavAction::OpenSettings => self.settings_screen.toggle(now),
            NavAction::OpenSettingsAt(page) => self.settings_screen.open_at(page, now),
            NavAction::OpenOnboarding => {
                self.settings_screen.close(now);
                self.onboarding.open(&self.services, now);
            }
            NavAction::GoBack => self.go_back(now, ctx),
            NavAction::OpenRail(entry) => {
                let screen = match entry {
                    RailEntry::Home => Screen::Home,
                    RailEntry::Section(section) => Screen::Section { section },
                    RailEntry::Library => Screen::Library,
                };
                self.nav.switch_top(screen);
            }
            NavAction::OpenCategory { id, items } => {
                self.nav.push(Screen::Category { id });
                self.category.seed(id, items);
            }
            NavAction::OpenDiscover { section, filters } => {
                self.nav.push(Screen::Discover { section });
                self.discover.seed(section, filters);
            }
            NavAction::OpenLibrary { list } => {
                self.nav.push(Screen::Library);
                self.library.seed(list);
            }
            NavAction::OpenSearch { query } => {
                self.search_bar.remember(&query);
                if let Some(db) = &self.services.db {
                    if let Err(error) = db_block_on(db.record_search(&query)) {
                        error!(%error, "failed to record search");
                    }
                }

                self.search.seed(query);
                if !matches!(self.nav.current(), Screen::Search) {
                    self.nav.push(Screen::Search);
                }
            }
            NavAction::OpenMedia { item } => {
                self.nav.push(Screen::Media {
                    kind: item.kind,
                    id: item.id,
                });
                self.media.seed(item);
            }
            NavAction::OpenPerson { person } => {
                self.nav.push(Screen::Person { id: person.id });
                self.person.seed(person);
            }
            NavAction::WatchTorrents => {
                let Screen::Media { kind, id } = self.nav.current() else {
                    return;
                };
                let Some(details) = self.media.ready() else {
                    return;
                };

                self.torrents
                    .ensure_open(details, &self.services.settings.parser.default_quality);
                self.nav.push(Screen::Torrents { kind, id });
            }
        }
    }

    fn go_back(&mut self, now: f64, ctx: &Context) {
        if self.settings_screen.on_back(now) {
            return;
        }

        if matches!(self.nav.current(), Screen::Player { .. }) {
            let played = self.player.stop(&mut self.services, ctx);
            self.nav.pop();
            if matches!(self.nav.current(), Screen::Torrents { .. }) {
                self.torrents.after_playback(&self.services, played);
            }
            return;
        }

        let on_media = matches!(self.nav.current(), Screen::Media { .. });
        if on_media && self.media.on_back() {
            return;
        }

        let on_discover = matches!(self.nav.current(), Screen::Discover { .. });
        if on_discover && self.discover.on_back(now) {
            return;
        }

        if self.torrents.on_back(now) {
            return;
        }

        if back_to_rail(ctx, self.nav.current()) {
            return;
        }

        let system_back = platform::profile(ctx).has_system_back();
        if self.nav.is_root() && system_back {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let leaving_torrents = matches!(self.nav.current(), Screen::Torrents { .. });
        self.nav.pop();
        if leaving_torrents {
            self.media.back_from_torrents();
        }
    }

    /// Escape (Back on TV) goes back unless something on screen takes it first.
    pub(super) fn back_on_escape(&mut self, ui: &Ui) -> Option<NavAction> {
        let escape = ui.input(|i| i.key_pressed(Key::Escape));

        // The wizard is dismissed with its own "Set up later" button only.
        if !escape || self.onboarding.is_open() {
            return None;
        }

        let ctx = ui.ctx();
        let on_player = matches!(self.nav.current(), Screen::Player { .. });

        // Escape closes an open dropdown; on TV, Back while typing only leaves the field.
        let popup_consumed = egui::Popup::is_any_open(ctx);
        let field_consumed = platform::profile(ctx).is_directional() && focus::was_editing(ctx);
        let search_consumed = self.search_bar.consume_escape(ctx);
        let player_consumed = on_player && self.player.consume_escape(ctx);
        let consumed = popup_consumed || field_consumed || search_consumed || player_consumed;

        (!consumed).then_some(NavAction::GoBack)
    }

    pub(super) fn take_pending_play(&mut self, ctx: &Context) {
        let req = self.torrents.take_play().or_else(|| self.media.take_play());
        let Some(req) = req else {
            return;
        };

        let id = req.card.id;
        let kind = req.card.kind;

        self.player.start(req, &mut self.services, ctx);
        self.nav.mark_focus(ctx.memory(|mem| mem.focused()));
        self.nav.push(Screen::Player { kind, id });
    }
}

/// On a page opened from the side menu, Back first returns the D-pad to the
/// menu's current item, as Android TV asks of apps with side navigation.
pub(crate) fn back_to_rail(ctx: &Context, screen: Screen) -> bool {
    if !platform::profile(ctx).is_directional() {
        return false;
    }

    if !screen.is_rail_destination() {
        return false;
    }

    if rail::had_focus(ctx) {
        return false;
    }

    let Some(entry) = screen.rail_entry() else {
        return false;
    };

    rail::focus(ctx, entry);
    true
}
