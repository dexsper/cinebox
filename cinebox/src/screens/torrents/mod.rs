mod state;

pub use state::{FilesPane, MovieBits, ReadyFiles, TorrentFileRow, TorrentHits, TorrentState};

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaDetails, MediaKind, TmdbId, tmdb_image_url, typograph};
use cinebox_parse::{
    AudioLang, QualityBand, SortMode, TriChoice, filtered_hits, hit_bitrate_mbps, season_options,
    voice_filter_options, year_options,
};
use cinebox_torrserver::AddSpec;
use egui::{
    Align, ComboBox, CornerRadius, Frame, Id, Layout, Modal, Rect, RichText, Sense, Stroke, Ui,
    UiBuilder, Vec2, pos2,
};
use egui_async::Bind;

use crate::jobs;
use crate::nav::NavAction;
use crate::services::Services;
use crate::theme::Theme;
use crate::widgets::{self, intro, poster, scroll};

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
        }
    }
}

impl TorrentsScreen {
    pub fn open(&mut self, details: &MediaDetails, now: f64) {
        self.state = Some(TorrentState::from_details(details));
        self.details = Some(details.clone());
        self.hits = Bind::new(true);
        self.opened = Bind::new(true);
        self.stream = Bind::new(true);
        self.intro_at = Some(now);
        self.stream_file = None;
        self.pending_play = None;
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

    pub fn hide(&mut self) {
        self.on_screen = false;
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
        details: Option<&MediaDetails>,
    ) -> Option<NavAction> {
        let now = ui.input(|i| i.time);
        let arriving = !self.on_screen;
        self.on_screen = true;

        if self.state.as_ref().is_none_or(|s| !s.matches(kind, id)) {
            if let Some(details) = details {
                self.open(details, now);
            } else {
                widgets::page_spinner(ui, theme);
                return None;
            }
        } else if arriving {
            self.intro_at = Some(now);
        }

        self.poll_binds(svc, theme, ui.ctx());
        self.take_stream(svc);

        let t = intro::t(self.intro_at, ui.input(|i| i.time));
        let mut retry = false;
        let mut pick = None;
        let mut pick_file = None;
        let mut retry_files = false;
        let mut close_files = false;

        let full = ui.available_rect_before_wrap();
        ui.advance_cursor_after_rect(full);

        let left_w = intro::lerp(theme.pad + theme.poster_w + 8.0, theme.explorer_left, t);
        let gap = 12.0;
        let left_rect = Rect::from_min_size(full.min, Vec2::new(left_w, full.height()));
        let right_rect = Rect::from_min_max(
            pos2(left_rect.right() + gap, full.top()),
            full.right_bottom(),
        );
        ui.painter().vline(
            left_rect.right() + gap * 0.5,
            full.y_range(),
            Stroke::new(1.0, theme.window_edge),
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
                    list_pane(ui, state, svc, theme, &mut retry, &mut pick, t);
                }
            },
        );

        if let Some(state) = &self.state
            && state.files.is_open()
        {
            files_modal(
                ui.ctx(),
                state,
                svc,
                theme,
                &mut pick_file,
                &mut retry_files,
                &mut close_files,
            );
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
        poster::rounded_image(ui, tex, Vec2::new(poster_w, poster_h), theme);
        ui.add_space(8.0);
        ui.label(
            RichText::new(typograph(&state.movie.title))
                .size(intro::lerp(32.0, 22.0, t))
                .color(theme.title),
        );
        if let Some(tagline) = state.movie.tagline.as_deref() {
            ui.label(
                RichText::new(typograph(tagline))
                    .size(15.0)
                    .color(theme.muted),
            );
        }
        if !state.movie.genres.is_empty() {
            ui.label(
                RichText::new(state.movie.genres.join(", "))
                    .size(13.0)
                    .color(theme.muted),
            );
        }
        let head = state.movie.head_line();
        if !head.is_empty() {
            ui.label(RichText::new(head).size(13.0).color(theme.muted));
        }
        if let Some(vote) = state.movie.vote.filter(|v| *v > 0.0) {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{vote:.1}"))
                        .size(18.0)
                        .color(theme.rate),
                );
                ui.label(RichText::new("TMDB").size(12.0).color(theme.muted));
            });
        }
        if let Some(overview) = state.movie.overview.as_deref() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(typograph(overview))
                    .size(intro::lerp(15.0, 13.5, t))
                    .color(theme.body),
            );
        }
    }

    fn poll_binds(&mut self, svc: &mut Services, theme: &Theme, ctx: &egui::Context) {
        let _ = theme;
        if let Some(state) = &self.state
            && matches!(state.hits, TorrentHits::Loading)
            && svc.settings.parser.url.trim().is_empty()
        {
            if let Some(state) = &mut self.state {
                state.hits = TorrentHits::Failed(Msg::NeedParser.en().to_owned());
            }
            return;
        }

        if let Some(state) = &self.state
            && matches!(state.hits, TorrentHits::Loading)
            && let Some(details) = self.details.clone()
        {
            let settings = svc.settings.clone();
            if let Some(result) = self
                .hits
                .read_or_request(move || jobs::load_torrents(settings, details))
            {
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
        }

        if let Some(result) = self.opened.read()
            && let Some(state) = &self.state
            && matches!(state.files, FilesPane::Loading)
        {
            match result {
                Ok(ready) => {
                    for file in &ready.files {
                        if let Some(url) = &file.still_url {
                            svc.images.request(
                                url.clone(),
                                false,
                                svc.settings.interface.use_system_proxy,
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
    }

    fn take_stream(&mut self, svc: &mut Services) {
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
        let _ = svc;
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
            state.files = FilesPane::Failed(Msg::NeedTorrServer.en().to_owned());
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

    fn retry_files(&mut self, svc: &Services) {
        let Some(state) = &mut self.state else {
            return;
        };
        let Some(spec) = state.pending_add.clone() else {
            return;
        };
        if svc.settings.torrserver.url.trim().is_empty() {
            state.files = FilesPane::Failed(Msg::NeedTorrServer.en().to_owned());
            return;
        }
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
        if wait && let FilesPane::Ready(files) = &state.files {
            state.files = FilesPane::Preloading {
                files: files.clone(),
                file_id,
            };
        }
        self.stream_file = Some(file_id);
        self.stream.clear();
        let settings = svc.settings.clone();
        self.stream
            .request(jobs::wait_stream(settings, path, hash, file_id));
    }
}

fn list_pane(
    ui: &mut Ui,
    state: &mut TorrentState,
    svc: &Services,
    theme: &Theme,
    retry: &mut bool,
    pick: &mut Option<usize>,
    t: f32,
) {
    if t < 0.22 {
        return;
    }
    ui.horizontal(|ui| {
        combo(ui, "sort", &mut state.sort, SortMode::ALL);
        let filters_label = if state.filter.is_active() {
            format!("{} · on", Msg::Filters.en())
        } else {
            Msg::Filters.en().to_owned()
        };
        if ui
            .selectable_label(
                state.filters_open || state.filter.is_active(),
                filters_label,
            )
            .clicked()
        {
            state.filters_open = !state.filters_open;
        }
    });
    if state.filters_open {
        filters_panel(ui, state, svc, theme);
    }
    match &state.hits {
        TorrentHits::Loading => {
            widgets::page_spinner(ui, theme);
        }
        TorrentHits::Failed(error) => {
            ui.label(RichText::new(error).color(theme.err));
            if ui.button("Retry").clicked() {
                *retry = true;
            }
        }
        TorrentHits::Ready(hits) => {
            let visible: Vec<(usize, &cinebox_parse::TorrentHit)> =
                filtered_hits(hits, state.filter).collect();
            ui.label(
                RichText::new(format!("{} / {}", visible.len(), hits.len()))
                    .size(13.0)
                    .color(theme.label),
            );
            scroll::vertical(ui, "torrent-hits", |ui| {
                if visible.is_empty() {
                    ui.label(RichText::new(Msg::NoTorrents.en()).color(theme.muted));
                    return;
                }
                ui.spacing_mut().item_spacing.y = 8.0;
                for (index, hit) in visible {
                    hit_row(
                        ui,
                        hit,
                        state.kind,
                        state.runtime_minutes,
                        theme,
                        pick,
                        index,
                    );
                }
            });
        }
    }
}

fn filters_panel(ui: &mut Ui, state: &mut TorrentState, svc: &Services, theme: &Theme) {
    ui.add_space(8.0);
    Frame::new()
        .fill(theme.overlay)
        .corner_radius(8)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(Msg::FilterQuality.en());
            ui.horizontal_wrapped(|ui| {
                let selected = state.filter.quality;
                if chip(ui, "Any", selected.is_none()) {
                    state.filter.quality = None;
                }
                for band in QualityBand::ALL {
                    if chip(ui, band.label(), selected == Some(*band)) {
                        state.filter.quality = Some(*band);
                    }
                }
            });
            ui.label(Msg::FilterHdr.en());
            tri_row(ui, &mut state.filter.hdr);
            ui.label(Msg::FilterDolby.en());
            tri_row(ui, &mut state.filter.dolby);
            ui.label(Msg::FilterSubs.en());
            tri_row(ui, &mut state.filter.subs);

            let hits = match &state.hits {
                TorrentHits::Ready(hits) => hits.as_slice(),
                _ => &[],
            };
            let voices = voice_filter_options(hits, state.filter.voice);
            combo(ui, "voice", &mut state.filter.voice, &voices);
            combo(ui, "lang", &mut state.filter.lang, AudioLang::ALL);

            let years = year_options(hits, state.year, state.filter.year);
            if years.len() > 1 {
                ui.horizontal(|ui| {
                    ui.label(Msg::FilterYear.en());
                    if ui
                        .selectable_label(state.filter.year.is_none(), "Any")
                        .clicked()
                    {
                        state.filter.year = None;
                    }
                    for year in years {
                        if ui
                            .selectable_label(state.filter.year == Some(year), format!("{year}"))
                            .clicked()
                        {
                            state.filter.year = Some(year);
                        }
                    }
                });
            }
            if state.kind == MediaKind::Tv {
                let seasons = season_options(hits);
                if !seasons.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Season");
                        if ui
                            .selectable_label(state.filter.season.is_none(), "Any")
                            .clicked()
                        {
                            state.filter.season = None;
                        }
                        for season in seasons {
                            if ui
                                .selectable_label(
                                    state.filter.season == Some(season),
                                    format!("S{season}"),
                                )
                                .clicked()
                            {
                                state.filter.season = Some(season);
                            }
                        }
                    });
                }
            }
            if ui.button(Msg::FilterReset.en()).clicked() {
                state.filter = cinebox_parse::TorrentFilter::default();
            }
        });
    state.apply_filter_sort(svc.settings.player.default_quality);
}

fn hit_row(
    ui: &mut Ui,
    hit: &cinebox_parse::TorrentHit,
    kind: MediaKind,
    runtime: Option<u32>,
    theme: &Theme,
    pick: &mut Option<usize>,
    index: usize,
) {
    let response = Frame::new()
        .fill(theme.card)
        .corner_radius(theme.rounding(theme.radius_card))
        .inner_margin(egui::Margin::symmetric(12, 14))
        .show(ui, |ui| {
            ui.label(
                RichText::new(typograph(&hit.title))
                    .size(15.0)
                    .color(theme.title),
            );
            ui.add_space(10.0);
            let bitrate = format_bitrate(kind, hit_bitrate_mbps(hit, runtime));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    pill(ui, hit.size_label(), theme);
                    metrics_bar(
                        ui,
                        theme,
                        bitrate.as_deref(),
                        &hit.seeders.to_string(),
                        &hit.peers.to_string(),
                    );
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let date = if hit.published.is_empty() {
                            "—"
                        } else {
                            hit.published.as_str()
                        };
                        ui.label(RichText::new(date).size(12.0).color(theme.muted));
                        ui.label(RichText::new(&hit.tracker).size(12.0).color(theme.muted));
                        if hit.started {
                            ui.label(RichText::new(Msg::TagStarted.en()).color(theme.ok));
                        }
                    });
                });
            });
        })
        .response
        .interact(Sense::click());
    if response.clicked() && !hit.magnet.is_empty() {
        *pick = Some(index);
    }
}

fn files_modal(
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

fn combo<T: Copy + PartialEq + std::fmt::Display>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    options: &[T],
) {
    ComboBox::from_id_salt(id)
        .selected_text(value.to_string())
        .show_ui(ui, |ui| {
            for opt in options {
                ui.selectable_value(value, *opt, opt.to_string());
            }
        });
}

fn tri_row(ui: &mut Ui, value: &mut TriChoice) {
    ui.horizontal(|ui| {
        for choice in TriChoice::ALL {
            if chip(ui, choice.label(), *value == *choice) {
                *value = *choice;
            }
        }
    });
}

fn chip(ui: &mut Ui, label: &str, active: bool) -> bool {
    ui.selectable_label(active, label).clicked()
}

const FILES_MODAL_MIN_W: f32 = 720.0;
const FILES_MODAL_MIN_H: f32 = 480.0;
const FILES_MODAL_EDGE: f32 = 64.0;

fn files_modal_size(screen: Vec2) -> Vec2 {
    let max = (screen - Vec2::splat(FILES_MODAL_EDGE)).max(Vec2::splat(320.0));
    Vec2::new(
        (screen.x * 0.72).clamp(FILES_MODAL_MIN_W.min(max.x), max.x),
        (screen.y * 0.72).clamp(FILES_MODAL_MIN_H.min(max.y), max.y),
    )
}

const METRIC_VAL_H: f32 = 16.0;
const BITRATE_VAL_W: f32 = 36.0;
const COUNT_VAL_W: f32 = 40.0;

fn format_bitrate(kind: MediaKind, mbps: Option<f64>) -> Option<String> {
    if kind != MediaKind::Movie {
        return None;
    }
    Some(
        mbps.map(|mbps| format!("{mbps:.1}"))
            .unwrap_or_else(|| String::from("—")),
    )
}

fn metrics_bar(ui: &mut Ui, theme: &Theme, bitrate: Option<&str>, seeds: &str, leechers: &str) {
    Frame::new()
        .fill(theme.metric_bg)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.horizontal(|ui| {
                if let Some(bitrate) = bitrate {
                    metric_pair(ui, Msg::Bitrate.en(), bitrate, BITRATE_VAL_W, theme);
                }
                metric_pair(ui, Msg::Seeds.en(), seeds, COUNT_VAL_W, theme);
                metric_pair(ui, Msg::Leechers.en(), leechers, COUNT_VAL_W, theme);
            });
        });
}

fn metric_pair(ui: &mut Ui, label: &str, value: &str, value_w: f32, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(label).size(12.0).color(theme.muted));
        let (rect, _) = ui.allocate_exact_size(Vec2::new(value_w, METRIC_VAL_H), Sense::hover());
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.label(RichText::new(value).size(12.0).color(theme.title));
            },
        );
    });
}

fn pill(ui: &mut Ui, label: String, theme: &Theme) {
    Frame::new()
        .fill(theme.size_pill_bg)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(12.0).color(theme.size_pill_fg));
        });
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
    fn bitrate_only_for_movies() {
        assert_eq!(
            format_bitrate(MediaKind::Movie, Some(8.2)).as_deref(),
            Some("8.2")
        );
        assert_eq!(format_bitrate(MediaKind::Movie, None).as_deref(), Some("—"));
        assert_eq!(format_bitrate(MediaKind::Tv, Some(8.2)), None);
        assert_eq!(format_bitrate(MediaKind::Tv, None), None);
    }

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
