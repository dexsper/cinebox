//! Torrent explorer state.

use cinebox_core::i18n::Msg;
use cinebox_core::{MediaDetails, MediaKind, QualityBand, TmdbId};
use cinebox_parse::{SortMode, TorrentFilter, TorrentHit, filtered_hits, sort_hits};
use cinebox_torrserver::AddSpec;

#[derive(Debug, Clone)]
pub struct MovieBits {
    pub title: String,
    pub overview: Option<String>,
    pub year: Option<u16>,
    pub vote: Option<f32>,
    pub certification: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub number_of_seasons: Option<u32>,
    /// Precomputed "year, country | country" line for the left pane.
    pub head_line: String,
    /// Precomputed ", "-joined genres line; empty when there are none.
    pub genres_line: String,
}

impl MovieBits {
    pub fn from_details(details: &MediaDetails) -> Self {
        let genres: Vec<&str> = details.genres.iter().take(3).map(String::as_str).collect();

        Self {
            title: details.title.clone(),
            overview: details.overview.clone(),
            year: details.year,
            vote: details.vote,
            certification: details.certification.clone(),
            poster_path: details.poster_path.clone(),
            backdrop_path: details.backdrop_path.clone(),
            number_of_seasons: details.number_of_seasons,
            head_line: head_line(details.year, &details.countries),
            genres_line: genres.join(", "),
        }
    }
}

fn head_line(year: Option<u16>, countries: &[String]) -> String {
    let mut parts = Vec::new();

    if let Some(year) = year {
        parts.push(year.to_string());
    }

    if !countries.is_empty() {
        let countries = countries
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ");
        parts.push(countries);
    }

    parts.join(", ")
}

#[derive(Debug, Clone)]
pub enum TorrentHits {
    Loading,
    Ready(Vec<TorrentHit>),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TorrentFileRow {
    pub id: i32,
    pub path: String,
    pub length: u64,
    pub timecode: f64,
    pub number: u32,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub title: String,
    pub still_url: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub air_date: Option<String>,
}

/// "Season N · Episode M" line shared by the files modal and player playlist.
#[must_use]
pub fn season_episode_line(season: u32, episode: Option<u32>) -> String {
    let mut line = format!("{} {season}", Msg::Season.t());

    let Some(episode) = episode else {
        return line;
    };

    line.push_str(&format!("  ·  {} {episode}", Msg::Episode.t()));
    line
}

impl TorrentFileRow {
    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.timecode <= 0.0 {
            return 0.0;
        }
        let Some(mins) = self.runtime_minutes.filter(|n| *n > 0) else {
            return 0.0;
        };
        let duration = f64::from(mins) * 60.0;
        (self.timecode / duration).clamp(0.0, 1.0) as f32
    }
}

#[derive(Debug, Clone)]
pub struct ReadyFiles {
    pub hash: String,
    pub files: Vec<TorrentFileRow>,
    pub resume_id: Option<i32>,
    pub selected_id: Option<i32>,
    pub scroll_to_resume: bool,
}

impl ReadyFiles {
    pub fn from_rows(hash: String, resume_id: Option<i32>, files: Vec<TorrentFileRow>) -> Self {
        Self {
            hash,
            files,
            resume_id,
            selected_id: resume_id,
            scroll_to_resume: resume_id.is_some(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FilesPane {
    Closed,
    Loading,
    Failed(String),
    Ready(ReadyFiles),
}

impl FilesPane {
    #[must_use]
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    #[must_use]
    pub fn ready(&self) -> Option<&ReadyFiles> {
        match self {
            Self::Ready(files) => Some(files),
            Self::Closed | Self::Loading | Self::Failed(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TorrentState {
    pub kind: MediaKind,
    pub id: TmdbId,
    pub movie: MovieBits,
    pub year: Option<u16>,
    pub runtime_minutes: Option<u32>,
    pub hits: TorrentHits,
    pub filter: TorrentFilter,
    pub sort: SortMode,
    pub files: FilesPane,
    pub pick_gen: u64,
    pub pending_add: Option<AddSpec>,
    /// The (sort, filter) pair `visible` was computed for; `None` = dirty.
    pub view_key: Option<(SortMode, TorrentFilter)>,
    /// Indices into sorted `hits` that pass the current filter.
    pub visible: Vec<usize>,
}

impl TorrentState {
    pub fn from_details(details: &MediaDetails, default_quality: &[QualityBand]) -> Self {
        Self {
            kind: details.kind,
            id: details.id,
            movie: MovieBits::from_details(details),
            year: details.year,
            runtime_minutes: details.runtime_minutes,
            hits: TorrentHits::Loading,
            filter: TorrentFilter {
                quality: default_quality.to_vec(),
                ..TorrentFilter::default()
            },
            sort: SortMode::Popular,
            files: FilesPane::Closed,
            pick_gen: 0,
            pending_add: None,
            view_key: None,
            visible: Vec::new(),
        }
    }

    pub fn matches(&self, kind: MediaKind, id: TmdbId) -> bool {
        self.kind == kind && self.id == id
    }

    /// Replace the hit list and mark the sorted/filtered view dirty.
    pub fn set_hits(&mut self, hits: TorrentHits) {
        self.hits = hits;
        self.view_key = None;
    }

    /// Promote hits whose magnet is in local play history. Does not refetch.
    pub fn mark_local_hashes(&mut self, hashes: &[String]) {
        if hashes.is_empty() {
            return;
        }

        let TorrentHits::Ready(hits) = &mut self.hits else {
            return;
        };

        for hit in hits.iter_mut() {
            hit.mark_local(hashes);
        }

        // `local_rank` participates in the sort order.
        self.view_key = None;
    }

    /// Re-sort `hits` and recompute `visible`, but only when the hit list,
    /// sort mode, or filters actually changed since the last call.
    pub fn apply_filter_sort(&mut self) {
        let TorrentHits::Ready(hits) = &mut self.hits else {
            self.view_key = None;
            self.visible.clear();
            return;
        };

        let current = self
            .view_key
            .as_ref()
            .is_some_and(|(sort, filter)| *sort == self.sort && *filter == self.filter);

        if current {
            return;
        }

        sort_hits(hits, self.kind, self.sort);

        self.visible = filtered_hits(hits, &self.filter)
            .map(|(index, _)| index)
            .collect();
        self.view_key = Some((self.sort, self.filter.clone()));
    }
}
