//! Files modal and per-file rows.

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaKind, typograph};
use egui::{Align, CornerRadius, Frame, Id, Layout, Modal, RichText, Sense, Ui, Vec2};

use super::state::{FilesPane, ReadyFiles, TorrentState};
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, poster, scroll};

pub(super) fn files_modal(
    ctx: &egui::Context,
    state: &TorrentState,
    svc: &Services,
    theme: &Theme,
    pick_file: &mut Option<i32>,
    retry_files: &mut bool,
    close_files: &mut bool,
) {
    let screen = ctx.content_rect().size();
    let size = files_modal_size(screen);
    let modal = Modal::new(Id::new("torrent-files"))
        .backdrop_color(theme.overlay)
        .frame(
            Frame::new()
                .inner_margin(egui::Margin::same(20))
                .corner_radius(theme.rounding(theme.radius_dialog))
                .fill(theme.panel_elevated),
        )
        .show(ctx, |ui| {
            ui.set_min_size(size);
            ui.set_max_size(size);
            ui.label(
                RichText::new(Msg::TorrentFiles.en())
                    .size(18.0)
                    .color(theme.title),
            );
            ui.add_space(12.0);
            if let FilesPane::Preloading { files, file_id } = &state.files {
                let name = files
                    .files
                    .iter()
                    .find(|file| file.id == *file_id)
                    .map(|file| file.title.as_str())
                    .unwrap_or("");
                ui.label(
                    RichText::new(format!("{} {name}", Msg::Preloading.en())).color(theme.muted),
                );
                ui.add_space(8.0);
            }
            match &state.files {
                FilesPane::Closed | FilesPane::Loading => {
                    widgets::page_spinner(ui, theme);
                }
                FilesPane::Failed(error) => {
                    ui.label(RichText::new(error).color(theme.err));
                    if ui.button("Retry").clicked() {
                        *retry_files = true;
                    }
                }
                FilesPane::Ready(files) | FilesPane::Preloading { files, .. } => {
                    file_list(ui, state, files, svc, theme, pick_file);
                }
            }
        });
    if modal.should_close() {
        *close_files = true;
    }
}

fn file_list(
    ui: &mut Ui,
    state: &TorrentState,
    files: &ReadyFiles,
    svc: &Services,
    theme: &Theme,
    pick_file: &mut Option<i32>,
) {
    if files.files.is_empty() {
        ui.label(RichText::new(Msg::NoPlayableFiles.en()).color(theme.muted));
        return;
    }

    if files.resume_id.is_some() && files.selected_id == files.resume_id {
        ui.label(
            RichText::new(Msg::TagStarted.en())
                .size(12.0)
                .color(theme.ok),
        );
    }

    let serial = state.kind == MediaKind::Tv;
    let fallback = svc.images.poster_key(
        state.kind,
        state.id,
        state.movie.poster_path.as_deref(),
        svc.settings.tmdb.poster_size,
    );
    scroll::vertical(ui, "torrent-files", |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        let mut last_season: Option<Option<u32>> = None;
        let show_headers = serial && files.files.iter().any(|file| file.season.is_some());
        for file in &files.files {
            if show_headers && last_season != Some(file.season) {
                ui.label(
                    RichText::new(format!("{} {}", Msg::Season.en(), file.season.unwrap_or(1)))
                        .size(13.0)
                        .color(theme.muted),
                );
                last_season = Some(file.season);
            }

            let selected = files.selected_id == Some(file.id);
            let still = svc.images.slot(file.still_url.as_deref()).or(fallback);

            let bg = if selected {
                theme.card_selected
            } else {
                theme.panel
            };

            let response = Frame::new()
                .fill(bg)
                .corner_radius(6)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        poster::rounded_image(
                            ui,
                            still,
                            Vec2::new(theme.still_w, theme.still_h),
                            theme,
                        );
                        ui.vertical(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(typograph(&file.title))
                                        .size(16.0)
                                        .color(theme.title),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(cinebox_parse::format_bytes(file.length))
                                            .color(theme.muted_bright),
                                    );
                                });
                            });

                            if serial {
                                let mut line =
                                    format!("{} {}", Msg::Season.en(), file.season.unwrap_or(1));
                                if let Some(episode) = file.episode {
                                    line = format!("{line}  ·  {} {episode}", Msg::Episode.en());
                                } else {
                                    line = format!("{line}  ·  {}", file.number);
                                }
                                ui.label(RichText::new(line).size(13.0).color(theme.muted));
                            }

                            if let Some(air) = file.air_date.as_deref() {
                                ui.label(RichText::new(air).size(12.0).color(theme.muted));
                            }

                            ui.add_space(6.0);
                            progress(ui, file.progress(), theme);
                        });
                    });
                })
                .response
                .interact(Sense::click());
            if response.clicked() {
                *pick_file = Some(file.id);
            }
        }
    });
}

fn progress(ui: &mut Ui, value: f32, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 3.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), theme.progress_track);
    if value > 0.0 {
        let mut fill = rect;
        fill.max.x = rect.left() + rect.width() * value.clamp(0.0, 1.0);
        ui.painter()
            .rect_filled(fill, CornerRadius::same(3), theme.progress_fill);
    }
}

const FILES_MODAL_MIN_W: f32 = 720.0;
const FILES_MODAL_MIN_H: f32 = 480.0;
const FILES_MODAL_EDGE: f32 = 64.0;

pub(super) fn files_modal_size(screen: Vec2) -> Vec2 {
    let max = (screen - Vec2::splat(FILES_MODAL_EDGE)).max(Vec2::splat(320.0));
    Vec2::new(
        (screen.x * 0.72).clamp(FILES_MODAL_MIN_W.min(max.x), max.x),
        (screen.y * 0.72).clamp(FILES_MODAL_MIN_H.min(max.y), max.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_modal_keeps_min_size_inside_window() {
        let size = files_modal_size(Vec2::new(1280.0, 800.0));
        assert!(size.x >= FILES_MODAL_MIN_W);
        assert!(size.y >= FILES_MODAL_MIN_H);
        assert!(size.x <= 1280.0 - FILES_MODAL_EDGE);
        assert!(size.y <= 800.0 - FILES_MODAL_EDGE);

        let tight = files_modal_size(Vec2::new(800.0, 600.0));
        assert!(tight.x <= 800.0 - FILES_MODAL_EDGE);
        assert!(tight.y <= 600.0 - FILES_MODAL_EDGE);
        assert!(tight.x >= 320.0);
        assert!(tight.y >= 320.0);
    }
}
