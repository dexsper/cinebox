//! Playlist flyout: compact file rows (still, title, episode line, progress).

use cinebox_core::i18n::Msg;
use egui::{CornerRadius, Frame, RichText, ScrollArea, Sense, Ui, Vec2, vec2};

use crate::screens::torrents::TorrentFileRow;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{button, poster};

const STILL_SCALE: f32 = 0.66;

/// Rows for every file; clicking one returns its index.
pub fn paint(
    ui: &mut Ui,
    theme: &Theme,
    svc: &Services,
    files: &[TorrentFileRow],
    current: usize,
) -> Option<usize> {
    let mut jump = None;
    let max_h = ui.ctx().content_rect().height() * 0.55;
    let still = Vec2::new(theme.still_w * STILL_SCALE, theme.still_h * STILL_SCALE);

    ScrollArea::vertical()
        .max_height(max_h)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            for (index, file) in files.iter().enumerate() {
                if file_row(ui, theme, svc, file, still, index == current) {
                    jump = Some(index);
                }
            }
        });

    jump
}

fn file_row(
    ui: &mut Ui,
    theme: &Theme,
    svc: &Services,
    file: &TorrentFileRow,
    still: Vec2,
    current: bool,
) -> bool {
    let id = ui.id().with(("player-playlist-file", file.id));
    let idle = if current {
        theme.card_selected
    } else {
        theme.panel
    };
    let fill = button::fill_for_hover(ui, id, idle, theme.widget_hover);

    let shown = Frame::new()
        .fill(fill)
        .corner_radius(6)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                poster::rounded_image(ui, still, theme, || {
                    svc.images.slot(file.still_url.as_deref())
                });

                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        RichText::new(&file.title)
                            .size(theme.text_body)
                            .color(theme.title),
                    );

                    if let Some(line) = episode_line(file) {
                        ui.label(
                            RichText::new(line)
                                .size(theme.text_caption)
                                .color(theme.muted),
                        );
                    }

                    ui.add_space(4.0);
                    progress(ui, file.progress(), theme);
                });
            });
        });

    button::click_rect(ui, id, shown.response.rect).clicked()
}

fn episode_line(file: &TorrentFileRow) -> Option<String> {
    let season = file.season?;
    let mut line = format!("{} {season}", Msg::Season.t());

    if let Some(episode) = file.episode {
        line = format!("{line}  ·  {} {episode}", Msg::Episode.t());
    }

    Some(line)
}

fn progress(ui: &mut Ui, value: f32, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 3.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), theme.progress_track);

    if value <= 0.0 {
        return;
    }

    let mut fill = rect;
    fill.max.x = rect.left() + rect.width() * value.clamp(0.0, 1.0);
    ui.painter()
        .rect_filled(fill, CornerRadius::same(3), theme.progress_fill);
}
