//! Right-hand overlay drawer (settings, torrent filters).

use egui::{Area, Frame, Id, Margin, Order, Rect, Sense, Ui, UiBuilder, pos2};

use crate::theme::Theme;
use crate::widgets::intro;

const DRAWER_FRAC: f32 = 0.4;
const DRAWER_MIN: f32 = 340.0;
const DRAWER_MAX: f32 = 520.0;

#[derive(Clone, Debug, Default)]
pub struct Overlay {
    want_open: bool,
    anim_at: Option<f64>,
}

impl Overlay {
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.want_open
    }

    #[must_use]
    pub fn animating(&self, now: f64) -> bool {
        intro::running(self.anim_at, now) && self.visual_t(now) > 0.0
    }

    pub fn toggle(&mut self, now: f64) {
        if self.want_open {
            self.begin_close(now);
            return;
        }

        self.begin_open(now);
    }

    pub fn begin_open(&mut self, now: f64) {
        let current = self.visual_t(now);
        self.want_open = true;
        self.anim_at = Some(intro::started_at(now, current));
    }

    pub fn begin_close(&mut self, now: f64) {
        let current = self.visual_t(now);
        self.want_open = false;
        self.anim_at = Some(intro::started_at(now, 1.0 - current));
    }

    pub fn snap_shut(&mut self) {
        self.want_open = false;
        self.anim_at = None;
    }

    /// Escape / chrome Back. `true` if the drawer consumed it.
    pub fn on_back(&mut self, now: f64) -> bool {
        if !self.is_blocking(now) {
            return false;
        }

        if !self.want_open {
            return true;
        }

        self.begin_close(now);
        true
    }

    #[must_use]
    pub fn is_blocking(&self, now: f64) -> bool {
        self.want_open || self.visual_t(now) > 0.02
    }

    #[must_use]
    pub fn visual_t(&self, now: f64) -> f32 {
        let t = intro::t(self.anim_at, now);
        if self.want_open {
            return t;
        }

        1.0 - t
    }

    fn finish_close(&mut self, now: f64) {
        if self.want_open {
            return;
        }

        if intro::running(self.anim_at, now) {
            return;
        }

        self.anim_at = None;
    }

    pub fn paint(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        id: &'static str,
        mut content: impl FnMut(&mut Ui, &Theme),
    ) {
        let now = ui.input(|i| i.time);
        self.finish_close(now);

        let t = self.visual_t(now);
        if t <= 0.001 {
            return;
        }

        if self.animating(now) {
            ui.ctx().request_repaint();
        }

        let full = ui.ctx().content_rect();
        let body_top = full.top() + theme.title_bar_h;
        let body = Rect::from_min_max(pos2(full.left(), body_top), full.right_bottom());
        let width = (body.width() * DRAWER_FRAC).clamp(DRAWER_MIN, DRAWER_MAX);
        let shown = width * t;
        let drawer_left = body.right() - shown;
        let dim_rect = Rect::from_min_max(body.left_top(), pos2(drawer_left, body.bottom()));
        let drawer_rect = Rect::from_min_max(pos2(drawer_left, body.top()), body.right_bottom());
        let mut dim_clicked = false;

        Area::new(Id::new(id))
            .order(Order::Foreground)
            .fixed_pos(body.min)
            .constrain(false)
            .show(ui.ctx(), |ui| {
                ui.set_min_size(body.size());
                ui.set_clip_rect(body);

                if dim_rect.width() > 1.0 {
                    ui.painter().rect_filled(dim_rect, 0.0, theme.overlay_at(t));
                    let dim = ui.interact(dim_rect, Id::new((id, "dim")), Sense::click());
                    if dim.clicked() {
                        dim_clicked = true;
                    }
                }

                if drawer_rect.width() < 8.0 {
                    return;
                }

                ui.scope_builder(UiBuilder::new().max_rect(drawer_rect), |ui| {
                    ui.set_min_size(drawer_rect.size());
                    ui.set_max_size(drawer_rect.size());
                    Frame::new()
                        .fill(theme.panel_elevated)
                        .inner_margin(Margin::symmetric(20, 16))
                        .show(ui, |ui| {
                            content(ui, theme);
                        });
                });
            });

        if dim_clicked {
            self.begin_close(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_closed() {
        let overlay = Overlay::default();

        assert!(!overlay.is_open());
        assert!(!overlay.is_blocking(0.0));
        assert!((overlay.visual_t(0.0) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn toggle_opens_and_closes() {
        let mut overlay = Overlay::default();

        overlay.toggle(1.0);
        assert!(overlay.is_open());
        assert!(overlay.on_back(1.1));
        assert!(!overlay.is_open());
    }
}
