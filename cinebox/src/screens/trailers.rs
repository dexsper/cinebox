//! Trailer picker modal on the media card.

use cinebox_core::Trailer;
use egui::{Frame, Id, Modal, RichText, Vec2};
use egui_async::Bind;
use rust_i18n::t;
use tracing::warn;

use crate::jobs::{self, JobError};
use crate::screens::play::{PlayRequest, PlaySource, WatchCard};
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, button, poster, scroll};

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrailersPhase {
    List,
    Resolving,
    Failed,
}

pub struct TrailersModal {
    open: bool,
    phase: TrailersPhase,
    items: Vec<Trailer>,
    card: Option<WatchCard>,
    backdrop_path: Option<String>,
    resolve: Bind<cinebox_youtube::Playback, JobError>,
    pending_key: Option<String>,
    pending_title: String,
    error: Option<String>,
    pending_play: Option<PlayRequest>,
}

impl Default for TrailersModal {
    fn default() -> Self {
        let mut resolve = Bind::new(true);
        resolve.set_abort(true);

        Self {
            open: false,
            phase: TrailersPhase::List,
            items: Vec::new(),
            card: None,
            backdrop_path: None,
            resolve,
            pending_key: None,
            pending_title: String::new(),
            error: None,
            pending_play: None,
        }
    }
}

impl TrailersModal {
    pub fn open(&mut self, card: WatchCard, backdrop_path: Option<String>, items: Vec<Trailer>) {
        if items.is_empty() {
            return;
        }

        self.open = true;
        self.phase = TrailersPhase::List;
        self.items = items;
        self.card = Some(card);
        self.backdrop_path = backdrop_path;
        self.pending_key = None;
        self.pending_title.clear();
        self.error = None;
        self.resolve.clear();
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn close(&mut self) -> bool {
        if !self.open {
            return false;
        }

        self.open = false;
        self.phase = TrailersPhase::List;
        self.error = None;
        self.pending_key = None;
        self.resolve.clear();
        true
    }

    pub fn take_play(&mut self) -> Option<PlayRequest> {
        self.pending_play.take()
    }

    pub fn poll(&mut self) {
        if self.phase != TrailersPhase::Resolving {
            return;
        }

        let Some(outcome) = self.resolve.take() else {
            return;
        };

        match outcome {
            Ok(play) => self.finish_play(play),
            Err(error) => {
                warn!(
                    %error,
                    key = self.pending_key.as_deref(),
                    "trailer resolve failed"
                );
                self.phase = TrailersPhase::Failed;
                self.error = Some(error.to_string());
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, svc: &Services, theme: &Theme) {
        if !self.open {
            return;
        }

        if self.phase == TrailersPhase::Resolving {
            ctx.request_repaint();
        }

        let screen = ctx.content_rect().size();
        let size = trailers_modal_size(screen);
        let (pick, retry, should_close) = {
            let mut pick = None;
            let mut retry = false;
            let items = &self.items;
            let card = self.card.as_ref();
            let phase = self.phase;
            let error = self.error.as_deref();

            let modal = Modal::new(Id::new("media-trailers"))
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
                        RichText::new(t!("media.trailers").as_ref())
                            .font(theme.title_font(theme.text_subtitle))
                            .color(theme.title),
                    );
                    ui.add_space(12.0);

                    match phase {
                        TrailersPhase::List => {
                            if let Some(card) = card {
                                trailer_list(ui, card, items, svc, theme, &mut pick);
                            }
                        }
                        TrailersPhase::Resolving => widgets::page_spinner(ui, theme),
                        TrailersPhase::Failed => {
                            let failed = t!("common.failed");
                            let text = error.unwrap_or(failed.as_ref());
                            if widgets::page_error(ui, theme, text) {
                                retry = true;
                            }
                        }
                    }
                });

            (pick, retry, modal.should_close())
        };

        if should_close {
            self.close();
            return;
        }

        if retry {
            self.retry_resolve(svc);
            return;
        }

        let Some(index) = pick else {
            return;
        };

        let Some(trailer) = self.items.get(index) else {
            return;
        };

        let key = trailer.youtube_key.clone();
        let title = trailer.name.clone();
        self.start_resolve(svc, key, title);
    }

    fn retry_resolve(&mut self, svc: &Services) {
        let Some(key) = self.pending_key.clone() else {
            return;
        };

        let title = self.pending_title.clone();
        self.start_resolve(svc, key, title);
    }

    fn start_resolve(&mut self, svc: &Services, key: String, title: String) {
        self.pending_key = Some(key.clone());
        self.pending_title = title;
        self.phase = TrailersPhase::Resolving;
        self.error = None;

        let net = jobs::net_config(&svc.settings);
        self.resolve.abort();
        self.resolve.clear();
        self.resolve.request(jobs::resolve_youtube(key, net));
    }

    fn finish_play(&mut self, play: cinebox_youtube::Playback) {
        let Some(card) = self.card.clone() else {
            let _ = self.close();
            return;
        };

        let title = if self.pending_title.is_empty() {
            card.title.clone()
        } else {
            self.pending_title.clone()
        };

        self.pending_play = Some(PlayRequest {
            card,
            title,
            backdrop_path: self.backdrop_path.clone(),
            source: PlaySource::Youtube {
                video_url: play.video_url,
                audio_url: play.audio_url,
                http_header_fields: play.http_header_fields,
            },
        });

        let _ = self.close();
    }
}

fn trailer_list(
    ui: &mut egui::Ui,
    card: &WatchCard,
    items: &[Trailer],
    svc: &Services,
    theme: &Theme,
    pick: &mut Option<usize>,
) {
    if items.is_empty() {
        ui.label(RichText::new(t!("common.failed").as_ref()).color(theme.muted));
        return;
    }

    scroll::vertical(ui, "media-trailers-list", |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;

        for (index, trailer) in items.iter().enumerate() {
            let row_id = ui.id().with(("media-trailer", index));
            let bg = button::fill_for_hover(ui, row_id, theme.panel, theme.widget_hover);

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
                                svc.images.poster_key(
                                    card.kind,
                                    card.id,
                                    card.poster_path.as_deref(),
                                    svc.settings.tmdb.poster_size,
                                )
                            },
                        );

                        ui.vertical(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(
                                RichText::new(&trailer.name)
                                    .size(theme.text_section)
                                    .color(theme.title),
                            );
                        });
                    });
                });

            let response = button::click_rect(ui, row_id, shown.response.rect);
            if response.clicked() {
                *pick = Some(index);
            }
        }
    });
}

const TRAILERS_MODAL_MIN_W: f32 = 720.0;
const TRAILERS_MODAL_MIN_H: f32 = 480.0;
const TRAILERS_MODAL_EDGE: f32 = 64.0;

fn trailers_modal_size(screen: Vec2) -> Vec2 {
    let max = (screen - Vec2::splat(TRAILERS_MODAL_EDGE)).max(Vec2::splat(320.0));
    Vec2::new(
        (screen.x * 0.72).clamp(TRAILERS_MODAL_MIN_W.min(max.x), max.x),
        (screen.y * 0.72).clamp(TRAILERS_MODAL_MIN_H.min(max.y), max.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailers_modal_keeps_min_size_inside_window() {
        let size = trailers_modal_size(Vec2::new(1280.0, 800.0));
        assert!(size.x >= TRAILERS_MODAL_MIN_W);
        assert!(size.y >= TRAILERS_MODAL_MIN_H);
        assert!(size.x <= 1280.0 - TRAILERS_MODAL_EDGE);
        assert!(size.y <= 800.0 - TRAILERS_MODAL_EDGE);

        let tight = trailers_modal_size(Vec2::new(800.0, 600.0));
        assert!(tight.x <= 800.0 - TRAILERS_MODAL_EDGE);
        assert!(tight.y <= 600.0 - TRAILERS_MODAL_EDGE);
        assert!(tight.x >= 320.0);
        assert!(tight.y >= 320.0);
    }

    #[test]
    fn open_ignores_empty_list() {
        let mut modal = TrailersModal::default();
        modal.open(
            WatchCard {
                kind: cinebox_core::MediaKind::Movie,
                id: cinebox_core::TmdbId::new(1),
                title: String::from("Movie"),
                poster_path: None,
                year: None,
                vote: None,
            },
            None,
            Vec::new(),
        );

        assert!(!modal.is_open());
    }
}
