//! Torrent explorer coordinator.

mod files;
mod list;
mod state;

pub use state::{FilesPane, MovieBits, ReadyFiles, TorrentFileRow, TorrentHits, TorrentState};

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaDetails, MediaKind, QualityBand, TmdbId, tmdb_image_url};
use cinebox_torrserver::AddSpec;
use egui::{Align, Layout, Rect, RichText, Ui, UiBuilder, Vec2, pos2};
use egui_async::Bind;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, intro, poster, scroll};
use crate::widgets::drawer::Overlay;

pub struct TorrentsScreen {
    state: Option<TorrentState>,
    details: Option<MediaDetails>,
    hits: Bind<Vec<cinebox_parse::TorrentHit>, String>,
    opened: Bind<ReadyFiles, String>,
    stream: Bind<String, String>,
    intro_at: Option<f64>,
    on_screen: bool,
    stream_file: Option<i32>,
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
            stream: Bind::new(true),
            intro_at: None,
            on_screen: false,
            stream_file: None,
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
        self.stream = Bind::new(true);
        self.intro_at = None;
        self.stream_file = None;
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
            self.refresh_hits();
        }

        self.poll_hits(svc, ui.ctx());
        self.poll_opened(svc, ui.ctx());
        self.take_stream();

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

        if let Some(state) = &self.state {
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
            self.hits.clear();
            if let Some(state) = &mut self.state {
                state.hits = TorrentHits::Loading;
            }
        }
        if let Some(index) = pick {
            self.pick_torrent(svc, index);
        }
        if retry_files {
            self.retry_files(svc);
        }
        if let Some(file_id) = pick_file {
            self.pick_file(svc, file_id);
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
        let tex = svc.images.poster_key(
            state.kind,
            state.id,
            state.movie.poster_path.as_deref(),
            svc.settings.tmdb.poster_size,
        );

        let head = state.movie.head_line();
        let overview_size = theme.text_small * 1.5;

        scroll::vertical(ui, "torrent-movie", |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                poster::rounded_image(ui, tex, Vec2::new(poster_w, poster_h), theme);
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
            if !state.movie.genres.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(state.movie.genres.join(", "))
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
    fn refresh_hits(&mut self) {
        self.hits.clear();
        let Some(state) = &mut self.state else {
            return;
        };

        if matches!(state.hits, TorrentHits::Failed(_)) {
            state.hits = TorrentHits::Loading;
        }
    }

    fn poll_hits(&mut self, svc: &mut Services, ctx: &egui::Context) {
        if svc.settings.parser.url.trim().is_empty() {
            if let Some(state) = &mut self.state {
                state.hits = TorrentHits::Failed(Msg::NeedParser.t().to_owned());
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

        let settings = svc.settings.clone();
        let Some(result) = self
            .hits
            .read_or_request(move || jobs::load_torrents(settings, details))
        else {
            ctx.request_repaint();
            return;
        };

        match result {
            Ok(hits) => {
                if let Some(state) = &mut self.state {
                    state.hits = TorrentHits::Ready(hits.clone());
                }
            }
            Err(error) => {
                svc.toasts.error(error.clone(), ctx.input(|i| i.time));
                if let Some(state) = &mut self.state {
                    state.hits = TorrentHits::Failed(error.clone());
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
                for file in &ready.files {
                    if let Some(url) = &file.still_url {
                        svc.images.request(
                            url.clone(),
                            false,
                            svc.settings.general.use_system_proxy,
                        );
                    }
                }
                if let Some(state) = &mut self.state {
                    state.files = FilesPane::Ready(ready.clone());
                }
            }
            Err(error) => {
                svc.toasts.error(error.clone(), ctx.input(|i| i.time));
                if let Some(state) = &mut self.state {
                    state.files = FilesPane::Failed(error.clone());
                }
            }
        }
    }

    fn take_stream(&mut self) {
        let Some(result) = self.stream.read().clone() else {
            return;
        };
        let Some(file_id) = self.stream_file else {
            return;
        };
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if !matches!(
            state.files,
            FilesPane::Ready(_) | FilesPane::Preloading { .. }
        ) {
            return;
        }

        let url = match result {
            Ok(url) => url,
            Err(error) => {
                state.files = FilesPane::Failed(error);
                self.stream.clear();
                self.stream_file = None;
                return;
            }
        };

        let mut files = match &state.files {
            FilesPane::Ready(files) | FilesPane::Preloading { files, .. } => files.clone(),
            FilesPane::Closed | FilesPane::Loading | FilesPane::Failed(_) => return,
        };
        files.selected_id = Some(file_id);
        let file_index = files
            .files
            .iter()
            .position(|file| file.id == file_id)
            .unwrap_or(0);
        let row = files.files.get(file_index);
        let title = row
            .map(|file| file.title.clone())
            .unwrap_or_else(|| state.movie.title.clone());
        let start = row.map(|file| file.timecode).unwrap_or(0.0);
        let hash = files.hash.clone();
        let rows = files.files.clone();
        let kind = state.kind;
        let id = state.id;
        state.files = FilesPane::Ready(files);
        state.files.close();
        self.stream.clear();
        self.stream_file = None;
        self.pending_play = Some(crate::screens::play::PlayRequest {
            kind,
            id,
            title,
            hash,
            files: rows,
            file_index,
            url,
            start,
        });
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
        let settings = svc.settings.clone();
        let movie = state.movie.clone();
        let kind = state.kind;
        let id = state.id;
        let runtime = state.runtime_minutes;
        self.opened.clear();
        self.opened.request(jobs::open_magnet(
            settings,
            spec,
            movie,
            kind,
            id,
            runtime,
            svc.db.clone(),
        ));
    }

    fn pick_file(&mut self, svc: &Services, file_id: i32) {
        let Some(state) = &mut self.state else {
            return;
        };
        let wait = svc.settings.torrserver.wait_preload;
        let (path, hash) = {
            let Some(ready) = state.files.ready_or_preload() else {
                return;
            };
            let Some(file) = ready.files.iter().find(|file| file.id == file_id) else {
                return;
            };
            (file.path.clone(), ready.hash.clone())
        };
        if wait {
            if let FilesPane::Ready(files) = &state.files {
                state.files = FilesPane::Preloading {
                    files: files.clone(),
                    file_id,
                };
            }
        }

        self.stream_file = Some(file_id);
        self.stream.clear();
        let settings = svc.settings.clone();
        self.stream
            .request(jobs::wait_stream(settings, path, hash, file_id));
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

    #[test]
    fn refresh_hits_retries_failed() {
        let mut screen = TorrentsScreen::default();
        screen.state = Some(TorrentState {
            kind: MediaKind::Movie,
            id: TmdbId::new(1),
            movie: MovieBits {
                title: String::from("Dune"),
                overview: None,
                year: Some(2021),
                vote: None,
                genres: Vec::new(),
                countries: Vec::new(),
                certification: None,
                poster_path: None,
                backdrop_path: None,
                number_of_seasons: None,
            },
            year: Some(2021),
            runtime_minutes: None,
            hits: TorrentHits::Failed(String::from("down")),
            filter: cinebox_parse::TorrentFilter::default(),
            sort: cinebox_parse::SortMode::Popular,
            files: FilesPane::Closed,
            pick_gen: 0,
            pending_add: None,
        });

        screen.refresh_hits();

        assert!(matches!(
            screen.state.as_ref().map(|state| &state.hits),
            Some(TorrentHits::Loading)
        ));
    }
}
