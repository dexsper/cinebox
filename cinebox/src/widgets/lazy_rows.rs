//! A long column of rows that builds only the ones near the view.
//!
//! Each row is built once to learn its height; after that a row far from the
//! view only takes up its space, so the scroll range stays exact. Rows within
//! a screen of the view are still built: the D-pad moves only between widgets
//! built last frame.
//!
//! A row's widgets need ids of their own (`ui.id().with(...)`): auto ids count
//! the widgets built before them, which change as rows come and go.

use egui::{Rangef, Ui};

/// How far past the view, in view heights, rows are still built.
const NEAR_SCREENS: f32 = 1.0;

#[derive(Debug, Clone, Default)]
pub struct LazyRows {
    width: f32,
    /// Height of each row plus the spacing after it, once the row has been built.
    spans: Vec<Option<f32>>,
}

impl LazyRows {
    /// The rows changed (another order, another filter): heights are learned again.
    pub fn reset(&mut self) {
        self.spans.clear();
    }

    /// `row` builds the row at a position. `keep` is built wherever it is:
    /// egui drops focus from a widget that was not built in a frame.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        count: usize,
        keep: Option<usize>,
        mut row: impl FnMut(&mut Ui, usize),
    ) {
        // Text wraps differently at another width.
        let width = ui.available_width();
        if width != self.width {
            self.width = width;
            self.spans.clear();
        }

        self.spans.resize(count, None);

        let view = ui.clip_rect().y_range();
        let near = view.expand(view.span() * NEAR_SCREENS);
        let mut skipped = 0.0;

        for index in 0..count {
            let top = ui.cursor().top() + skipped;
            if let Some(span) = self.skips(index, top, near, keep) {
                skipped += span;
                continue;
            }

            ui.add_space(skipped);
            skipped = 0.0;

            let before = ui.cursor().top();
            row(ui, index);
            self.spans[index] = Some(ui.cursor().top() - before);
        }

        ui.add_space(skipped);
    }

    /// The space row `index` takes when it is left unbuilt this frame.
    fn skips(&self, index: usize, top: f32, near: Rangef, keep: Option<usize>) -> Option<f32> {
        let span = self.spans[index]?;
        if keep == Some(index) {
            return None;
        }

        let rows = Rangef::new(top, top + span);
        if rows.intersects(near) {
            return None;
        }

        Some(span)
    }
}
