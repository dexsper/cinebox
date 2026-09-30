//! Stand-ins for screens that cannot load until a service is configured.

use cinebox_core::Settings;
use egui::{OpenUrl, Ui};
use egui_material_icons::icons::{ICON_KEY, ICON_OPEN_IN_NEW, ICON_SETTINGS};
use rust_i18n::t;

use crate::i18n::tr;
use crate::nav::{NavAction, SettingsPage};
use crate::settings_input::{KeyHint, tmdb_key_hint};
use crate::theme::Theme;
use crate::widgets::button::Tone;
use crate::widgets::page_state::{PageAction, PageState, page_state};

pub const TMDB_KEY_URL: &str = "https://www.themoviedb.org/settings/api";

/// Why the catalog cannot load with the current TMDB key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TmdbKeyProblem {
    Missing,
    AccessToken,
    BadFormat,
}

#[must_use]
pub fn tmdb_key_problem(settings: &Settings) -> Option<TmdbKeyProblem> {
    let key = settings.tmdb.api_key.expose();
    if key.is_empty() {
        return Some(TmdbKeyProblem::Missing);
    }

    match tmdb_key_hint(key)? {
        KeyHint::AccessToken => Some(TmdbKeyProblem::AccessToken),
        KeyHint::BadFormat => Some(TmdbKeyProblem::BadFormat),
    }
}

#[derive(Clone, Copy)]
enum TmdbChoice {
    OpenSettings,
    GetKey,
}

/// Explains what is wrong with the key and links to the fix.
pub fn tmdb_key(ui: &mut Ui, theme: &Theme, problem: TmdbKeyProblem) -> Option<NavAction> {
    let (title, body) = match problem {
        TmdbKeyProblem::Missing => ("gate.tmdb_title", "gate.tmdb_body"),
        TmdbKeyProblem::AccessToken => ("error.tmdb_access_token", "settings.tmdb_key_token"),
        TmdbKeyProblem::BadFormat => ("gate.tmdb_bad_title", "settings.tmdb_key_format"),
    };
    let title = tr(title);
    let body = tr(body);

    let state = PageState {
        icon: ICON_KEY,
        accent: theme.title,
        title: &title,
        body: Some(&body),
        detail: None,
        actions: vec![
            PageAction {
                icon: ICON_SETTINGS,
                label: t!("gate.tmdb_open_settings").into_owned(),
                tone: Tone::Primary,
                value: TmdbChoice::OpenSettings,
            },
            PageAction {
                icon: ICON_OPEN_IN_NEW,
                label: t!("gate.tmdb_get_key").into_owned(),
                tone: Tone::Secondary,
                value: TmdbChoice::GetKey,
            },
        ],
    };

    match page_state(ui, theme, &state)? {
        TmdbChoice::OpenSettings => Some(NavAction::OpenSettingsAt(SettingsPage::Tmdb)),
        TmdbChoice::GetKey => {
            ui.ctx().open_url(OpenUrl::new_tab(TMDB_KEY_URL));
            None
        }
    }
}
