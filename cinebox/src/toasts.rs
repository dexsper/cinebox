//! Non-blocking toasts. Colors come from [`crate::theme::Theme`].

use egui::{Align2, Area, Color32, Frame, Id, Order, RichText, Sense};

use crate::theme::Theme;

const HOLD_SECS: f64 = 4.0;
const FADE_SECS: f64 = 0.4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    #[allow(dead_code)]
    Info,
    #[allow(dead_code)]
    Success,
    Error,
}

struct Toast {
    id: u64,
    kind: ToastKind,
    text: String,
    born: f64,
}

#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next_id: u64,
}

impl Toasts {
    #[allow(dead_code)]
    pub fn info(&mut self, text: impl Into<String>, now: f64) {
        self.push(ToastKind::Info, text, now);
    }

    #[allow(dead_code)]
    pub fn success(&mut self, text: impl Into<String>, now: f64) {
        self.push(ToastKind::Success, text, now);
    }

    pub fn error(&mut self, text: impl Into<String>, now: f64) {
        self.push(ToastKind::Error, text, now);
    }

    fn push(&mut self, kind: ToastKind, text: impl Into<String>, now: f64) {
        self.next_id = self.next_id.wrapping_add(1);
        self.items.push(Toast {
            id: self.next_id,
            kind,
            text: text.into(),
            born: now,
        });
    }

    pub fn show(&mut self, ctx: &egui::Context, theme: &Theme) {
        let now = ctx.input(|i| i.time);
        self.items.retain(|toast| now - toast.born < HOLD_SECS);
        if self.items.is_empty() {
            return;
        }

        Area::new(Id::new("cinebox-toasts"))
            .anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .order(Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                let mut dismiss = None;
                for toast in self.items.iter().rev() {
                    let age = now - toast.born;
                    let alpha = if age > HOLD_SECS - FADE_SECS {
                        ((HOLD_SECS - age) / FADE_SECS).clamp(0.0, 1.0) as f32
                    } else {
                        1.0
                    };

                    let accent = fade(kind_color(toast.kind, theme), alpha);
                    let bg = fade(theme.panel_elevated, alpha);
                    let fg = fade(theme.title, alpha);

                    let response = Frame::new()
                        .fill(bg)
                        .stroke(egui::Stroke::new(2.0, accent))
                        .corner_radius(theme.rounding(theme.radius_card))
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new(&toast.text).color(fg).size(14.0));
                        })
                        .response
                        .interact(Sense::click());

                    if response.clicked() {
                        dismiss = Some(toast.id);
                    }
                }

                if let Some(id) = dismiss {
                    self.items.retain(|toast| toast.id != id);
                }
            });

        if self
            .items
            .iter()
            .any(|t| now - t.born > HOLD_SECS - FADE_SECS)
        {
            ctx.request_repaint();
        }
    }
}

fn kind_color(kind: ToastKind, theme: &Theme) -> Color32 {
    match kind {
        ToastKind::Info => theme.toast_info,
        ToastKind::Success => theme.toast_ok,
        ToastKind::Error => theme.toast_err,
    }
}

fn fade(color: Color32, alpha: f32) -> Color32 {
    let [r, g, b, a] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (f32::from(a) * alpha).round() as u8)
}
