//! Centered empty and error states: icon, title, explanation, buttons.

use egui::{Align2, Color32, FontId, Id, Label, RichText, Sense, Ui, Vec2, vec2};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{ICON_ERROR, ICON_REFRESH, ICON_SETTINGS};
use rust_i18n::t;

use super::button::{self, Opts, Tone};
use crate::errors::UserError;
use crate::nav::{NavAction, SettingsPage};
use crate::theme::Theme;

const BADGE: f32 = 64.0;
const BADGE_GAP: f32 = 16.0;
/// Height of the icon badge and its gap above the title.
#[cfg(test)]
pub const BADGE_BLOCK: f32 = BADGE + BADGE_GAP;
const BUTTON_MIN_W: f32 = 140.0;
const BUTTON_GAP: f32 = 8.0;

pub struct PageAction<T> {
    pub icon: MaterialIcon,
    pub label: String,
    pub tone: Tone,
    pub value: T,
}

pub struct PageState<'a, T> {
    pub icon: MaterialIcon,
    pub accent: Color32,
    pub title: &'a str,
    pub body: Option<&'a str>,
    /// Technical small print (error chain), selectable for copying.
    pub detail: Option<&'a str>,
    pub actions: Vec<PageAction<T>>,
}

/// Returns the `value` of the clicked button.
pub fn page_state<T: Copy>(ui: &mut Ui, theme: &Theme, state: &PageState<'_, T>) -> Option<T> {
    super::in_remaining(ui, |ui| state_body(ui, theme, state)).flatten()
}

/// What the viewer picked on an [`error_page`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorChoice {
    Retry,
    Fix(SettingsPage),
}

impl ErrorChoice {
    /// Navigation for "Open settings"; Retry is up to the screen.
    #[must_use]
    pub fn nav(self) -> Option<NavAction> {
        let Self::Fix(page) = self else {
            return None;
        };

        Some(NavAction::OpenSettingsAt(page))
    }
}

/// Error with Retry, plus "Open settings" when a settings page can fix it.
pub fn error_page(ui: &mut Ui, theme: &Theme, error: &UserError) -> Option<ErrorChoice> {
    let mut actions = Vec::new();
    if let Some(page) = error.fix {
        actions.push(PageAction {
            icon: ICON_SETTINGS,
            label: t!("common.open_settings").into_owned(),
            tone: Tone::Primary,
            value: ErrorChoice::Fix(page),
        });
    }

    actions.push(PageAction {
        icon: ICON_REFRESH,
        label: t!("common.retry").into_owned(),
        tone: Tone::Secondary,
        value: ErrorChoice::Retry,
    });

    let state = PageState {
        icon: ICON_ERROR,
        accent: theme.err,
        title: &error.title,
        body: error.hint.as_deref(),
        detail: error.detail.as_deref(),
        actions,
    };

    page_state(ui, theme, &state)
}

fn state_body<T: Copy>(ui: &mut Ui, theme: &Theme, state: &PageState<'_, T>) -> Option<T> {
    icon_badge(ui, theme, state.icon, state.accent);
    ui.add_space(BADGE_GAP);
    ui.label(title_text(theme, state.title));

    if let Some(body) = state.body {
        ui.add_space(6.0);
        ui.label(body_text(theme, body));
    }

    if let Some(detail) = state.detail {
        ui.add_space(8.0);
        ui.add(Label::new(detail_text(theme, detail)).selectable(true));
    }

    if state.actions.is_empty() {
        return None;
    }

    ui.add_space(20.0);
    action_row(ui, theme, &state.actions)
}

fn title_text(theme: &Theme, text: &str) -> RichText {
    let font = theme.title_font(theme.text_display);
    RichText::new(text).font(font).color(theme.title)
}

fn body_text(theme: &Theme, text: &str) -> RichText {
    let size = theme.text_body;
    RichText::new(text).size(size).color(theme.muted_bright)
}

fn detail_text(theme: &Theme, text: &str) -> RichText {
    let size = theme.text_caption;
    RichText::new(text).size(size).color(theme.muted)
}

fn icon_badge(ui: &mut Ui, theme: &Theme, icon: MaterialIcon, accent: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(BADGE), Sense::hover());
    let center = rect.center();
    let glyph = FontId::new(theme.text_icon_lg * 1.4, icon.font_family());

    let painter = ui.painter();
    painter.circle_filled(center, BADGE / 2.0, theme.input_bg);
    painter.text(center, Align2::CENTER_CENTER, icon.codepoint, glyph, accent);
}

/// Buttons side by side, centered. Each button's width is only known after
/// it is laid out, so the row width is remembered from the previous frame.
fn action_row<T: Copy>(ui: &mut Ui, theme: &Theme, actions: &[PageAction<T>]) -> Option<T> {
    let id = ui.id().with("page-actions");
    let remembered = ui.ctx().data(|d| d.get_temp::<f32>(id));
    let row_w = remembered.unwrap_or(ui.available_width());
    let indent = ((ui.available_width() - row_w) / 2.0).max(0.0);

    let mut picked = None;
    let row = ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        for action in actions {
            let opts = tone_opts(action.tone);
            if button::icon_label(ui, theme, action.icon, &action.label, opts) {
                picked = Some(action.value);
            }
        }
    });

    let measured = row.response.rect.width() - indent;
    remember_row_width(ui, id, remembered, measured);
    picked
}

fn remember_row_width(ui: &Ui, id: Id, remembered: Option<f32>, measured: f32) {
    let same = remembered.is_some_and(|width| (width - measured).abs() < 0.5);
    if same {
        return;
    }

    ui.ctx().data_mut(|d| d.insert_temp(id, measured));
    ui.ctx().request_repaint();
}

fn tone_opts(tone: Tone) -> Opts {
    let size = vec2(BUTTON_MIN_W, super::combo::HEIGHT);
    match tone {
        Tone::Primary => Opts::primary(size),
        Tone::Secondary => Opts::secondary(size),
    }
}
