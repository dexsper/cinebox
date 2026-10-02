//! Header search field: query, submit, and recent-search dropdown.

use cinebox_core::SEARCH_HISTORY_LIMIT;
use egui::emath::GuiRounding;
use egui::{
    Align, Align2, Area, Color32, FontId, Frame, Id, Key, LayerId, Layout, Margin, Order, Rect,
    Sense, Stroke, StrokeKind, TextEdit, Ui, UiBuilder, WidgetInfo, WidgetType, pos2, vec2,
};
use egui_material_icons::icons::{ICON_MIC, ICON_SCHEDULE, ICON_SEARCH};
use rust_i18n::t;

use crate::nav::NavAction;
use crate::platform::{self, SpeechEvent, SpeechRequest, TextAction, TextInputSpec, TextPurpose};
use crate::theme::Theme;
use crate::widgets::button::{self, pointing};
use crate::widgets::{flyout, focus};

pub const SEARCH_W: f32 = 380.0;
pub const SEARCH_H: f32 = 28.0;
const ICON_W: f32 = SEARCH_H;
const ROW_H: f32 = SEARCH_H;
const MIN_SHOW: f32 = 8.0;
const HISTORY_PAD: f32 = 4.0;
const HISTORY_PAD_BOTTOM: f32 = 8.0;
const HISTORY_GAP: f32 = 2.0;
const ROW_PAD_X: f32 = 6.0;
const ROW_ICON_GAP: f32 = 8.0;
const MIC_GAP: f32 = 6.0;
const LISTEN_PULSE_SECS: f64 = 1.6;

const EDIT_ID: &str = "cinebox-header-search";
const FIELD_LAYER_ID: &str = "cinebox-header-search-layer";

#[derive(Default)]
pub struct SearchBar {
    pub query: String,
    history: Vec<String>,
    history_open: bool,
    listening: bool,
}

impl SearchBar {
    #[must_use]
    pub fn with_history(history: Vec<String>) -> Self {
        Self {
            query: String::new(),
            history,
            history_open: false,
            listening: false,
        }
    }

    pub fn remember(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }

        let query_lower = query.to_lowercase();
        self.history
            .retain(|item| item.to_lowercase() != query_lower);
        self.history.insert(0, query.to_owned());
        self.history.truncate(SEARCH_HISTORY_LIMIT);
        self.query = query.to_owned();
        self.history_open = false;
    }

    /// The remote's search key: listen when the device can, otherwise start typing.
    pub fn start_search(&mut self, ctx: &egui::Context) {
        if platform::device(ctx).speech_input_available() {
            self.start_voice(ctx);
            return;
        }

        self.history_open = true;
        ctx.memory_mut(|mem| mem.request_focus(Id::new(EDIT_ID)));
    }

    /// Speech input as it goes: what is heard fills the field, and the final
    /// result is searched for right away.
    pub fn on_speech(&mut self, event: SpeechEvent) -> Option<NavAction> {
        match event {
            SpeechEvent::Listening => self.listening = true,
            SpeechEvent::Partial(text) => self.query = text,
            SpeechEvent::Final(text) => return self.heard(&text),
            SpeechEvent::Ended => self.listening = false,
        }

        None
    }

    fn heard(&mut self, text: &str) -> Option<NavAction> {
        self.listening = false;
        self.history_open = false;
        let action = submit(text)?;
        self.query = text.trim().to_owned();

        Some(action)
    }

    fn start_voice(&mut self, ctx: &egui::Context) {
        let request = SpeechRequest {
            language: rust_i18n::locale().to_string(),
        };

        self.listening = true;
        self.history_open = false;
        platform::device(ctx).start_speech_input(&request);
    }

    fn toggle_voice(&mut self, ctx: &egui::Context) {
        if self.listening {
            self.stop_voice(ctx);
            return;
        }

        self.start_voice(ctx);
    }

    fn stop_voice(&mut self, ctx: &egui::Context) {
        self.listening = false;
        platform::device(ctx).stop_speech_input();
    }

    /// Stop listening, or close the history panel and surrender focus. `true`
    /// when Escape was used.
    pub fn consume_escape(&mut self, ctx: &egui::Context) -> bool {
        if self.listening {
            self.stop_voice(ctx);
            return true;
        }

        let focused = ctx.memory(|mem| mem.has_focus(Id::new(EDIT_ID)));
        if !self.history_open && !focused {
            return false;
        }

        self.history_open = false;
        ctx.memory_mut(|mem| mem.surrender_focus(Id::new(EDIT_ID)));

        true
    }

    pub fn show(&mut self, ui: &mut Ui, theme: &Theme, rect: Rect) -> Option<NavAction> {
        if rect.width() < MIN_SHOW || rect.height() < MIN_SHOW {
            return None;
        }

        let rect = rect.round_ui();
        let ctx = ui.ctx().clone();
        let (rect, mic) = split_off_mic(&ctx, rect);
        if let Some(mic) = mic {
            if mic_button(ui, theme, mic, self.listening) {
                self.toggle_voice(&ctx);
            }
        }

        let attached = self.history_open && !self.history.is_empty();
        let mut picked = None;

        let layer = Area::new(Id::new(FIELD_LAYER_ID))
            .order(Order::Foreground)
            .constrain(false)
            .fixed_pos(rect.min)
            .show(&ctx, |ui| {
                if attached {
                    return self.paint_open(ui, theme, rect, &mut picked);
                }

                let (local, _) = ui.allocate_exact_size(rect.size(), Sense::hover());

                self.paint_closed(ui, theme, local)
            });

        if attached && flyout::pressed_outside(&ctx, layer.response.rect, rect) {
            self.history_open = false;
        }

        if let Some(action) = layer.inner {
            self.history_open = false;
            return Some(action);
        }

        if let Some(query) = picked {
            self.query = query.clone();
            self.history_open = false;
            return Some(NavAction::OpenSearch { query });
        }

        self.close_if_unfocused(&ctx, layer.response.rect);

        None
    }

    fn paint_open(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        field: Rect,
        picked: &mut Option<String>,
    ) -> Option<NavAction> {
        let history = self.history.clone();
        let body_h = history_body_h(history.len());
        let size = vec2(field.width(), field.height() + body_h);
        let (combined, _) = ui.allocate_exact_size(size, Sense::hover());

        ui.painter().rect(
            combined,
            theme.rounding(theme.radius_card),
            theme.input_bg,
            Stroke::new(1.0, theme.selection),
            StrokeKind::Inside,
        );

        let field_rect = Rect::from_min_size(combined.min, field.size());
        let submitted = self.paint_contents(ui, theme, field_rect);

        let panel = Rect::from_min_max(pos2(combined.left(), field_rect.bottom()), combined.max);
        let rows = Rect::from_min_max(
            pos2(panel.left() + HISTORY_PAD, panel.top() + HISTORY_PAD),
            pos2(
                panel.right() - HISTORY_PAD,
                panel.bottom() - HISTORY_PAD_BOTTOM,
            ),
        );

        ui.scope_builder(
            UiBuilder::new()
                .max_rect(rows)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.set_width(rows.width());
                ui.spacing_mut().item_spacing.y = HISTORY_GAP;
                for query in &history {
                    if history_row(ui, theme, query) {
                        *picked = Some(query.clone());
                    }
                }
            },
        );

        submitted
    }

    fn paint_closed(&mut self, ui: &mut Ui, theme: &Theme, rect: Rect) -> Option<NavAction> {
        let focused = ui.ctx().memory(|mem| mem.has_focus(Id::new(EDIT_ID)));
        let edge = if focused || self.history_open {
            theme.selection
        } else {
            theme.window_edge
        };

        ui.painter().rect(
            rect,
            theme.rounding(theme.radius_card),
            theme.input_bg,
            Stroke::new(1.0, edge),
            StrokeKind::Inside,
        );

        self.paint_contents(ui, theme, rect)
    }

    fn paint_contents(&mut self, ui: &mut Ui, theme: &Theme, rect: Rect) -> Option<NavAction> {
        let directional = platform::profile(ui.ctx()).is_directional();
        let icon_rect = Rect::from_min_size(rect.min, vec2(ICON_W, rect.height()));
        let icon_clicked = search_icon(ui, theme, icon_rect, directional);
        if icon_clicked {
            if let Some(action) = submit(&self.query) {
                return Some(action);
            }

            self.history_open = true;
            ui.ctx()
                .memory_mut(|mem| mem.request_focus(Id::new(EDIT_ID)));
        }

        let edit_rect = Rect::from_min_max(pos2(rect.left() + ICON_W, rect.top()), rect.max);
        // On a remote the whole bar is one stop: OK opens the keyboard, whose
        // search key submits.
        let stop_rect = if directional { rect } else { edit_rect };
        let gate = focus::edit_gate(ui, stop_rect, Id::new(EDIT_ID));
        let spec = TextInputSpec {
            purpose: TextPurpose::Text,
            action: TextAction::Search,
        };
        platform::text_input::declare(ui.ctx(), Id::new(EDIT_ID), spec);
        // On TV the history drops down as soon as the D-pad reaches the field.
        if gate.stop.as_ref().is_some_and(egui::Response::has_focus) {
            self.history_open = true;
        }

        let mut submitted = None;
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(edit_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().interact_size.y = rect.height();
                let hint = if self.listening {
                    t!("search.listening")
                } else {
                    t!("search.placeholder")
                };
                let edit = ui.add(
                    TextEdit::singleline(&mut self.query)
                        .id(Id::new(EDIT_ID))
                        .interactive(gate.interactive)
                        .desired_width(f32::INFINITY)
                        .vertical_align(Align::Center)
                        .margin(Margin::ZERO)
                        .hint_text(hint.as_ref())
                        .frame(Frame::NONE)
                        .text_color(theme.title),
                );

                edit.widget_info(|| WidgetInfo::labeled(WidgetType::TextEdit, true, hint.as_ref()));
                focus::edit_done(ui, &edit);
                if edit.gained_focus() || edit.clicked() {
                    self.history_open = true;
                }

                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                let should_submit = enter && (edit.has_focus() || edit.lost_focus());

                if should_submit {
                    submitted = submit(&self.query);
                }
            },
        );

        submitted
    }

    fn close_if_unfocused(&mut self, ctx: &egui::Context, layer: Rect) {
        let focused = ctx.memory(|mem| mem.has_focus(Id::new(EDIT_ID)));
        if focused {
            return;
        }

        // The D-pad is on the field or a history row.
        let field_layer = LayerId::new(Order::Foreground, Id::new(FIELD_LAYER_ID));
        let in_field = focus::focused_layer(ctx) == Some(field_layer);
        if in_field && platform::profile(ctx).is_directional() {
            return;
        }

        let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) else {
            self.history_open = false;
            return;
        };

        if layer.contains(pos) {
            return;
        }

        self.history_open = false;
    }
}

fn submit(query: &str) -> Option<NavAction> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }

    Some(NavAction::OpenSearch {
        query: query.to_owned(),
    })
}

fn history_body_h(count: usize) -> f32 {
    if count == 0 {
        return 0.0;
    }

    let rows = count as f32 * ROW_H;
    let gaps = (count - 1) as f32 * HISTORY_GAP;

    rows + gaps + HISTORY_PAD + HISTORY_PAD_BOTTOM
}

/// Clickable only with a pointer; a remote reaches the bar as a whole.
fn search_icon(ui: &mut Ui, theme: &Theme, rect: Rect, directional: bool) -> bool {
    paint_search_icon(ui, theme, rect);
    if directional {
        return false;
    }

    let id = Id::new("cinebox-search-icon");
    pointing(ui.interact(rect, id, Sense::click())).clicked()
}

fn paint_search_icon(ui: &Ui, theme: &Theme, rect: Rect) {
    let font = FontId::new(theme.text_icon, ICON_SEARCH.font_family());
    let galley = ui
        .painter()
        .layout_no_wrap(ICON_SEARCH.codepoint.to_owned(), font, theme.muted);

    let pos = Align2::CENTER_CENTER
        .anchor_size(rect.center(), galley.size())
        .min;

    ui.painter().galley(pos, galley, theme.muted);
}

/// The microphone sits beside the bar, not inside it, so the D-pad can step
/// between the two.
fn split_off_mic(ctx: &egui::Context, rect: Rect) -> (Rect, Option<Rect>) {
    if !platform::device(ctx).speech_input_available() {
        return (rect, None);
    }

    let side = rect.height();
    let mic = Rect::from_min_size(pos2(rect.right() - side, rect.top()), vec2(side, side));
    let mut field = rect;
    field.max.x = mic.left() - MIC_GAP;

    (field, Some(mic))
}

fn mic_button(ui: &mut Ui, theme: &Theme, rect: Rect, listening: bool) -> bool {
    let hint = t!("search.voice");
    let response = button::click_rect(ui, Id::new("cinebox-search-voice"), rect);
    let info = || WidgetInfo::selected(WidgetType::Button, true, listening, hint.as_ref());
    response.widget_info(info);

    let (fill, icon_color) = mic_colors(ui, theme, &response, listening);

    let radius = theme.rounding(theme.radius_card);
    let edge = Stroke::new(1.0, theme.window_edge);
    ui.painter()
        .rect(rect, radius, fill, edge, StrokeKind::Inside);

    let font = FontId::new(theme.text_icon, ICON_MIC.font_family());
    let glyph = ICON_MIC.codepoint;
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        font,
        icon_color,
    );

    response.on_hover_text(hint.as_ref()).clicked()
}

/// While listening the button glows in the accent color, breathing so it
/// reads as live.
fn mic_colors(
    ui: &Ui,
    theme: &Theme,
    response: &egui::Response,
    listening: bool,
) -> (Color32, Color32) {
    if listening {
        let phase = ui.input(|i| i.time) * std::f64::consts::TAU / LISTEN_PULSE_SECS;
        let breath = 0.75 + 0.25 * phase.cos() as f32;
        ui.ctx().request_repaint();
        return (theme.btn_primary_bg.gamma_multiply(breath), theme.btn_primary_fg);
    }

    if focus::lit(response) {
        return (theme.widget_hover, theme.muted_bright);
    }

    (theme.input_bg, theme.muted_bright)
}

fn history_row(ui: &mut Ui, theme: &Theme, query: &str) -> bool {
    let size = vec2(ui.available_width(), ROW_H);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let id = ui.id().with(query);
    let response = pointing(ui.interact(rect, id, Sense::click()));
    let fill = button::fill_for_hover(ui, id, Color32::TRANSPARENT, theme.widget_hover);
    ui.painter()
        .rect_filled(rect, theme.rounding(theme.radius_card), fill);

    let icon_font = FontId::new(theme.text_icon, ICON_SCHEDULE.font_family());
    let icon =
        ui.painter()
            .layout_no_wrap(ICON_SCHEDULE.codepoint.to_owned(), icon_font, theme.muted);
    let text = ui.painter().layout_no_wrap(
        query.to_owned(),
        theme.ui_font(theme.text_body),
        theme.title,
    );

    let center_y = rect.center().y;
    let icon_pos = pos2(
        rect.left() + ROW_PAD_X,
        center_y - icon.mesh_bounds.center().y,
    );
    let text_pos = pos2(
        icon_pos.x + icon.size().x + ROW_ICON_GAP,
        center_y - text.mesh_bounds.center().y,
    );

    ui.painter().galley(icon_pos, icon, theme.muted);
    ui.painter().galley(text_pos, text, theme.title);

    let enabled = response.enabled();
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, query));

    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_trims_and_rejects_blank() {
        assert!(submit("   ").is_none());
        assert_eq!(
            submit("  dune  "),
            Some(NavAction::OpenSearch {
                query: String::from("dune"),
            })
        );
    }

    #[test]
    fn remember_is_mru_and_case_insensitive() {
        let mut bar = SearchBar::default();
        bar.remember("Dune");
        bar.remember("Alien");
        bar.remember("dune");

        assert_eq!(bar.history, ["dune", "Alien"]);
        assert_eq!(bar.query, "dune");

        for index in 0..SEARCH_HISTORY_LIMIT + 2 {
            bar.remember(&format!("q{index}"));
        }

        assert_eq!(bar.history.len(), SEARCH_HISTORY_LIMIT);
        assert_eq!(bar.history[0], format!("q{}", SEARCH_HISTORY_LIMIT + 1));
    }
}
