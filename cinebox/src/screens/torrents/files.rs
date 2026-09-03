//! Files modal and per-file rows.

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaKind, TmdbId};
use egui::{Align, CornerRadius, Frame, Id, Layout, Modal, RichText, Sense, Ui, Vec2};

use super::state::{FilesPane, ReadyFiles, TorrentState, season_episode_line};
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, button, poster, scroll};

pub(super) fn files_modal(
    ctx: &egui::Context,
    state: &mut TorrentState,
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
                RichText::new(Msg::TorrentFiles.t())
                    .font(theme.title_font(theme.text_subtitle))
                    .color(theme.title),
            );
            ui.add_space(12.0);
            match &mut state.files {
                FilesPane::Closed | FilesPane::Loading => {
                    widgets::page_spinner(ui, theme);
                }
                FilesPane::Failed(error) => {
                    if widgets::page_error(ui, theme, error) {
                        *retry_files = true;
                    }
                }
                FilesPane::Ready(files) => {
                    let media = FileListMedia {
                        kind: state.kind,
                        id: state.id,
                        poster_path: state.movie.poster_path.as_deref(),
                    };
                    file_list(ui, media, files, svc, theme, pick_file);
                }
            }
        });
    if modal.should_close() {
        *close_files = true;
    }
}

#[derive(Clone, Copy)]
struct FileListMedia<'a> {
    kind: MediaKind,
    id: TmdbId,
    poster_path: Option<&'a str>,
}

fn file_list(
    ui: &mut Ui,
    media: FileListMedia<'_>,
    files: &mut ReadyFiles,
    svc: &Services,
    theme: &Theme,
    pick_file: &mut Option<i32>,
) {
    if files.files.is_empty() {
        ui.label(RichText::new(Msg::NoPlayableFiles.t()).color(theme.muted));
        return;
    }

    if files.resume_id.is_some() && files.selected_id == files.resume_id {
        ui.label(
            RichText::new(Msg::TagStarted.t())
                .size(theme.text_caption)
                .color(theme.ok),
        );
    }

    let serial = media.kind == MediaKind::Tv;
    let mut scrolled = false;
    scroll::vertical(ui, ("torrent-files", files.hash.as_str()), |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        let mut last_season: Option<Option<u32>> = None;
        let show_headers = serial && files.files.iter().any(|file| file.season.is_some());
        for file in &files.files {
            if show_headers && last_season != Some(file.season) {
                ui.label(
                    RichText::new(format!("{} {}", Msg::Season.t(), file.season.unwrap_or(1)))
                        .size(theme.text_small)
                        .color(theme.muted),
                );
                last_season = Some(file.season);
            }

            let selected = files.selected_id == Some(file.id);
            let row_id = ui.id().with(("torrent-file", file.id));
            let idle = if selected {
                theme.card_selected
            } else {
                theme.panel
            };

            let bg = button::fill_for_hover(ui, row_id, idle, theme.widget_hover);

            let shown = Frame::new()
                .fill(bg)
                .corner_radius(6)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        poster::rounded_image(
                            ui,
                            Vec2::new(theme.still_w, theme.still_h),
                            theme,
                            || {
                                svc.images.slot(file.still_url.as_deref()).or_else(|| {
                                    svc.images.poster_key(
                                        media.kind,
                                        media.id,
                                        media.poster_path,
                                        svc.settings.tmdb.poster_size,
                                    )
                                })
                            },
                        );

                        ui.vertical(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&file.title)
                                        .size(theme.text_section)
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
                                let season = file.season.unwrap_or(1);
                                let mut line = season_episode_line(season, file.episode);
                                if file.episode.is_none() {
                                    line = format!("{line}  ·  {}", file.number);
                                }

                                ui.label(
                                    RichText::new(line)
                                        .size(theme.text_small)
                                        .color(theme.muted),
                                );
                            }

                            if let Some(air) = file.air_date.as_deref() {
                                ui.label(
                                    RichText::new(air)
                                        .size(theme.text_caption)
                                        .color(theme.muted),
                                );
                            }

                            ui.add_space(6.0);
                            progress(ui, file.progress(), theme);
                        });
                    });
                });

            if files.scroll_to_resume && selected {
                shown.response.scroll_to_me(Some(Align::Center));
                scrolled = true;
            }

            let response = button::click_rect(ui, row_id, shown.response.rect);
            if response.clicked() {
                *pick_file = Some(file.id);
            }
        }
    });

    if scrolled {
        files.scroll_to_resume = false;
    }
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

    #[test]
    fn ready_files_scrolls_to_resume_once() {
        let with_resume = ReadyFiles::from_rows(String::from("hash"), Some(7), Vec::new());
        assert!(with_resume.scroll_to_resume);
        assert_eq!(with_resume.selected_id, Some(7));

        let fresh = ReadyFiles::from_rows(String::from("hash"), None, Vec::new());
        assert!(!fresh.scroll_to_resume);
        assert!(fresh.selected_id.is_none());
    }
}
