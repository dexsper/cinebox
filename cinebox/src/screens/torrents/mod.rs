//! Torrent explorer coordinator.

mod files;
mod list;
mod state;

pub use state::{
    FilesPane, MovieBits, ReadyFiles, TorrentFileRow, TorrentHits, TorrentState,
    season_episode_line,
};

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaDetails, MediaKind, QualityBand, TmdbId, tmdb_image_url};
use cinebox_torrserver::AddSpec;
use egui::{Align, Layout, Rect, RichText, Ui, UiBuilder, Vec2, pos2};
use egui_async::Bind;

use crate::jobs::{self, JobError};
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::drawer::Overlay;
use crate::widgets::{self, intro, poster, scroll};

pub struct TorrentsScreen {
    state: Option<TorrentState>,
    details: Option<MediaDetails>,
    hits: Bind<Vec<cinebox_parse::TorrentHit>, JobError>,
    opened: Bind<ReadyFiles, JobError>,
    local_hashes: Bind<(MediaKind, TmdbId, Vec<String>), String>,
    intro_at: Option<f64>,
    on_screen: bool,
    pending_play: Option<crate::screens::play::PlayRequest>,
    filters: Overlay,
}

impl Default for TorrentsScreen {
    fn default() -> Self {
        Self {
            state: None,
            details: None,
            hits: Bind::new(true),
            opened: Bind::new(true),
            local_hashes: Bind::new(true),
            intro_at: None,
            on_screen: false,
            pending_play: None,
            filters: Overlay::default(),
        }
    }
}

impl TorrentsScreen {
    pub fn ensure_open(&mut self, details: &MediaDetails, default_quality: &[QualityBand]) {
        if let Some(state) = &mut self.state {
            let same = state.matches(details.kind, details.id);
            if same {
                state.movie = MovieBits::from_details(details);
                self.details = Some(details.clone());
                return;
            }
        }

        self.state = Some(TorrentState::from_details(details, default_quality));
        self.details = Some(details.clone());
        self.hits = Bind::new(true);
        self.opened = Bind::new(true);
        self.intro_at = None;
        self.pending_play = None;
        self.filters.snap_shut();
    }

    pub fn take_play(&mut self) -> Option<crate::screens::play::PlayRequest> {
        self.pending_play.take()
    }

    pub fn leave_files_if_open(&mut self) -> bool {
        let Some(state) = &mut self.state else {
            return false;
        };
        if !state.files.is_open() {
            return false;
        }

        state.files.close();
        true
    }

    pub fn on_back(&mut self, now: f64) -> bool {
        if self.leave_files_if_open() {
            return true;
        }

        self.filters.on_back(now)
    }

    pub fn hide(&mut self) {
        self.on_screen = false;
        self.filters.snap_shut();
    }

    pub fn intro_animating(&self, now: f64) -> bool {
        intro::running(self.intro_at, now)
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        svc: &mut Services,
        theme: &Theme,
        kind: MediaKind,
        id: TmdbId,
    ) -> Option<NavAction> {
        let now = ui.input(|i| i.time);
        let arriving = !self.on_screen;
        self.on_screen = true;

        let matches = self
            .state
            .as_ref()
            .is_some_and(|state| state.matches(kind, id));
        if !matches {
            widgets::page_spinner(ui, theme);
            return None;
        }

        if arriving {
            self.intro_at = Some(now);
            self.retag_local_hits(svc);
        }
        self.apply_local_hits();

        self.poll_hits(svc, ui.ctx());
        self.poll_opened(svc, ui.ctx());

        let t = intro::t(self.intro_at, ui.input(|i| i.time));
        let mut retry = false;
        let mut pick = None;
        let mut pick_file = None;
        let mut retry_files = false;
        let mut close_files = false;

        let full = ui.available_rect_before_wrap();
        ui.advance_cursor_after_rect(full);

        let left_w = theme.explorer_left;
        let gap = 12.0;
        let left_rect = Rect::from_min_size(full.min, Vec2::new(left_w, full.height()));
        let right_rect = Rect::from_min_max(
            pos2(left_rect.right() + gap, full.top()),
            full.right_bottom(),
        );

        ui.scope_builder(
            UiBuilder::new()
                .max_rect(left_rect)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.set_clip_rect(ui.clip_rect().intersect(left_rect));
                self.left_pane(ui, svc, theme, t);
            },
        );
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(right_rect)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.set_clip_rect(ui.clip_rect().intersect(right_rect));
                if let Some(state) = &mut self.state {
                    list::list_pane(
                        ui,
                        state,
                        theme,
                        &mut retry,
                        &mut pick,
                        t,
                        &mut self.filters,
                    );
                }
            },
        );

        let mut overlay = std::mem::take(&mut self.filters);
        if let Some(state) = &mut self.state {
            overlay.paint(ui, theme, "cinebox-torrent-filters", |ui, theme| {
                list::filters_drawer(ui, state, theme);
            });
        }
        self.filters = overlay;

        if let Some(state) = &mut self.state {
            if state.files.is_open() {
                files::files_modal(
                    ui.ctx(),
                    state,
                    svc,
                    theme,
                    &mut pick_file,
                    &mut retry_files,
                    &mut close_files,
                );
            }
        }

        if retry {
            self.refresh_hits();
        }

        if let Some(index) = pick {
            self.pick_torrent(svc, index);
        }

        if retry_files {
            self.retry_files(svc);
        }

        if let Some(file_id) = pick_file {
            self.pick_file(file_id);
        }

        if close_files {
            self.leave_files_if_open();
        }

        None
    }

    fn left_pane(&self, ui: &mut Ui, svc: &Services, theme: &Theme, t: f32) {
        let Some(state) = &self.state else {
            return;
        };

        let poster_w = intro::lerp(theme.poster_w, theme.explorer_poster_w, t);
        let poster_h = intro::lerp(theme.poster_h, theme.explorer_poster_h, t);

        let head = state.movie.head_line.as_str();
        let overview_size = theme.text_small * 1.5;

        scroll::vertical(ui, "torrent-movie", |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let poster =
                    poster::rounded_image(ui, Vec2::new(poster_w, poster_h), theme, || {
                        svc.images.poster_key(
                            state.kind,
                            state.id,
                            state.movie.poster_path.as_deref(),
                            svc.settings.tmdb.poster_size,
                        )
                    });
                if svc.is_watched(state.kind, state.id) {
                    poster::watched_badge(ui, poster, theme);
                }

                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width());
                    if !head.is_empty() {
                        ui.label(
                            RichText::new(head)
                                .size(theme.text_section)
                                .color(theme.muted),
                        );
                        ui.add_space(8.0);
                    }
                    widgets::rating::row(
                        ui,
                        theme,
                        state.movie.vote,
                        state.movie.certification.as_deref(),
                    );
                });
            });

            ui.add_space(10.0);
            ui.label(
                RichText::new(&state.movie.title)
                    .font(theme.title_font(intro::lerp(
                        theme.text_explorer_from,
                        theme.text_display,
                        t,
                    )))
                    .color(theme.title),
            );

            if !state.movie.genres_line.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&state.movie.genres_line)
                        .size(theme.text_small)
                        .color(theme.muted),
                );
            }

            let Some(overview) = state.movie.overview.as_deref() else {
                return;
            };

            ui.add_space(18.0);
            ui.label(
                RichText::new(overview)
                    .size(overview_size)
                    .color(theme.body),
            );
        });
    }
}

impl TorrentsScreen {
    /// Kick off the async watch-hash lookup; `apply_local_hits` picks up the
    /// result on a later frame.
    fn retag_local_hits(&mut self, svc: &Services) {
        let Some(state) = &self.state else {
            return;
        };

        let Some(db) = svc.db.clone() else {
            return;
        };

        let kind = state.kind;
        let id = state.id;
        self.local_hashes.refresh(async move {
            let hashes = db
                .watch_release_hashes(kind, id)
                .await
                .map_err(|error| error.to_string())?;

            Ok((kind, id, hashes))
        });
    }

    fn apply_local_hits(&mut self) {
        let Some(Ok((kind, id, hashes))) = self.local_hashes.take() else {
            return;
        };

        let Some(state) = &mut self.state else {
            return;
        };

        if !state.matches(kind, id) {
            return;
        }

        state.mark_local_hashes(&hashes);
    }

    fn refresh_hits(&mut self) {
        self.hits.clear();
        let Some(state) = &mut self.state else {
            return;
        };

        if matches!(state.hits, TorrentHits::Failed(_)) {
            state.set_hits(TorrentHits::Loading);
        }
    }

    fn poll_hits(&mut self, svc: &mut Services, ctx: &egui::Context) {
        if svc.settings.parser.url.trim().is_empty() {
            if let Some(state) = &mut self.state {
                state.set_hits(TorrentHits::Failed(Msg::NeedParser.t().to_owned()));
            }
            return;
        }

        let bind_settled = self.hits.read().is_some();
        let waiting = self
            .state
            .as_ref()
            .is_some_and(|state| matches!(state.hits, TorrentHits::Loading));

        if bind_settled && !waiting {
            return;
        }

        let Some(details) = self.details.clone() else {
            return;
        };

        let parser = jobs::ParserCtx::from(&svc.settings);
        let torr = jobs::TorrCtx::from(&svc.settings);
        let db = svc.db.clone();
        let Some(result) = self
            .hits
            .read_or_request(move || jobs::load_torrents(parser, torr, details, db))
        else {
            ctx.request_repaint();
            return;
        };

        match result {
            Ok(hits) => {
                if let Some(state) = &mut self.state {
                    state.set_hits(TorrentHits::Ready(hits.clone()));
                }
            }
            Err(error) => {
                let error = error.to_string();
                svc.toasts.error(error.clone(), ctx.input(|i| i.time));

                if let Some(state) = &mut self.state {
                    state.set_hits(TorrentHits::Failed(error));
                }
            }
        }
    }

    fn poll_opened(&mut self, svc: &mut Services, ctx: &egui::Context) {
        let loading = self
            .state
            .as_ref()
            .is_some_and(|state| matches!(state.files, FilesPane::Loading));
        if !loading {
            return;
        }

        let Some(result) = self.opened.read() else {
            return;
        };

        match result {
            Ok(ready) => {
                if let Some(state) = &mut self.state {
                    state.files = FilesPane::Ready(ready.clone());
                }
            }
            Err(error) => {
                let error = error.to_string();
                svc.toasts.error(error.clone(), ctx.input(|i| i.time));
                if let Some(state) = &mut self.state {
                    state.files = FilesPane::Failed(error);
                }
            }
        }
    }

    fn pick_torrent(&mut self, svc: &Services, index: usize) {
        let Some(state) = &mut self.state else {
            return;
        };
        let TorrentHits::Ready(hits) = &state.hits else {
            return;
        };
        let Some(hit) = hits.get(index) else {
            return;
        };
        if hit.magnet.is_empty() {
            return;
        }
        if svc.settings.torrserver.url.trim().is_empty() {
            state.files = FilesPane::Failed(Msg::NeedTorrServer.t().to_owned());
            return;
        }

        let category = match state.kind {
            MediaKind::Tv => "tv",
            MediaKind::Movie | MediaKind::Person => "movie",
        };
        let poster = tmdb_image_url(
            state.movie.poster_path.as_deref(),
            svc.settings.tmdb.poster_size.tmdb_path(),
        )
        .unwrap_or_default();
        let spec = AddSpec {
            link: hit.magnet.clone(),
            title: state.movie.title.clone(),
            poster,
            category: category.to_owned(),
            save_to_db: svc.settings.torrserver.save_to_db,
        };
        self.request_open(svc, spec);
    }

    fn retry_files(&mut self, svc: &Services) {
        let Some(state) = &self.state else {
            return;
        };
        let Some(spec) = state.pending_add.clone() else {
            return;
        };
        if svc.settings.torrserver.url.trim().is_empty() {
            if let Some(state) = &mut self.state {
                state.files = FilesPane::Failed(Msg::NeedTorrServer.t().to_owned());
            }
            return;
        }

        self.request_open(svc, spec);
    }

    fn request_open(&mut self, svc: &Services, spec: AddSpec) {
        let Some(state) = &mut self.state else {
            return;
        };

        state.pending_add = Some(spec.clone());
        state.pick_gen += 1;
        state.files = FilesPane::Loading;
        let torr = jobs::TorrCtx::from(&svc.settings);
        let tmdb = jobs::TmdbCtx::from(&svc.settings);
        let movie = state.movie.clone();
        let target = jobs::OpenTarget {
            kind: state.kind,
            id: state.id,
            runtime_minutes: state.runtime_minutes,
        };
        self.opened.clear();
        self.opened.request(jobs::open_magnet(
            torr,
            tmdb,
            spec,
            movie,
            target,
            svc.db.clone(),
        ));
    }

    /// Hand off to the player immediately; buffering (if any) is the player's job.
    fn pick_file(&mut self, file_id: i32) {
        let Some(state) = &mut self.state else {
            return;
        };
        let Some(ready) = state.files.ready() else {
            return;
        };

        let Some(file_index) = ready.files.iter().position(|file| file.id == file_id) else {
            return;
        };

        let Some(file) = ready.files.get(file_index) else {
            return;
        };

        self.pending_play = Some(crate::screens::play::PlayRequest {
            card: crate::screens::play::WatchCard {
                kind: state.kind,
                id: state.id,
                title: state.movie.title.clone(),
                poster_path: state.movie.poster_path.clone(),
                year: state.movie.year,
                vote: state.movie.vote,
            },
            title: file.title.clone(),
            hash: ready.hash.clone(),
            files: ready.files.clone(),
            file_index,
            start: file.timecode,
            backdrop_path: state.movie.backdrop_path.clone(),
        });

        state.files.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intro_finishes_and_restarts_after_hide() {
        let mut screen = TorrentsScreen {
            intro_at: Some(0.0),
            on_screen: true,
            ..TorrentsScreen::default()
        };
        assert!(!screen.intro_animating(10.0));

        screen.hide();
        assert!(!screen.on_screen);

        screen.on_screen = true;
        screen.intro_at = Some(10.0);
        assert!(screen.intro_animating(10.05));
    }

    fn test_state(hits: TorrentHits) -> TorrentState {
        TorrentState {
            kind: MediaKind::Movie,
            id: TmdbId::new(1),
            movie: MovieBits {
                title: String::from("Dune"),
                overview: None,
                year: Some(2021),
                vote: None,
                certification: None,
                poster_path: None,
                backdrop_path: None,
                number_of_seasons: None,
                head_line: String::from("2021"),
                genres_line: String::new(),
            },
            year: Some(2021),
            runtime_minutes: None,
            hits,
            filter: cinebox_parse::TorrentFilter::default(),
            sort: cinebox_parse::SortMode::Popular,
            files: FilesPane::Closed,
            pick_gen: 0,
            pending_add: None,
            view_key: None,
            visible: Vec::new(),
        }
    }

    #[test]
    fn refresh_hits_retries_failed() {
        let state = Some(test_state(TorrentHits::Failed(String::from("down"))));

        let mut screen = TorrentsScreen {
            state,
            ..TorrentsScreen::default()
        };

        screen.refresh_hits();

        assert!(matches!(
            screen.state.as_ref().map(|state| &state.hits),
            Some(TorrentHits::Loading)
        ));
    }

    #[test]
    fn mark_local_hashes_promotes_ready_hits() {
        let hash = String::from("dddddddddddddddddddddddddddddddddddddddd");
        let hit = cinebox_parse::TorrentHit::new(
            cinebox_parse::Listing {
                title: String::from("Dune.2021.1080p"),
                tracker: String::from("rutracker"),
                size_bytes: 1,
                seeders: 1,
                peers: 0,
                magnet: format!("magnet:?xt=urn:btih:{hash}&dn=dune"),
                published: String::new(),
            },
            None,
            &[],
            &[],
        );
        let mut state = test_state(TorrentHits::Ready(vec![hit]));

        state.mark_local_hashes(&[hash]);

        let TorrentHits::Ready(hits) = &state.hits else {
            panic!("hits should stay ready");
        };
        assert_eq!(hits[0].local_rank, Some(0));
    }
}
