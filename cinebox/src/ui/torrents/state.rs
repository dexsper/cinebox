//! Torrent explorer state and filter events.

use cinebox_core::{DefaultQuality, MediaDetails, MediaKind, TmdbId};
use cinebox_parse::{
    AudioLang, QualityBand, SortMode, TorrentFilter, TorrentHit, TriChoice, VoiceFilter, sort_hits,
};
use cinebox_torrserver::AddSpec;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    ToggleFilters,
    FilterQuality(Option<QualityBand>),
    FilterHdr(TriChoice),
    FilterDolby(TriChoice),
    FilterSubs(TriChoice),
    FilterVoice(VoiceFilter),
    FilterLang(AudioLang),
    FilterSeason(Option<u32>),
    FilterYear(Option<u16>),
    FilterReset,
    Sort(SortMode),
    Pick(usize),
    CloseFiles,
    KeepOpen,
    PickFile(i32),
    RetryFiles,
}

#[derive(Debug, Clone)]
pub struct MovieBits {
    pub title: String,
    pub tagline: Option<String>,
    pub overview: Option<String>,
    pub year: Option<u16>,
    pub vote: Option<f32>,
    pub genres: Vec<String>,
    pub countries: Vec<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub number_of_seasons: Option<u32>,
}

impl MovieBits {
    pub fn from_details(details: &MediaDetails) -> Self {
        Self {
            title: details.title.clone(),
            tagline: details.tagline.clone(),
            overview: details.overview.clone(),
            year: details.year,
            vote: details.vote,
            genres: details.genres.iter().take(3).cloned().collect(),
            countries: details.countries.clone(),
            poster_path: details.poster_path.clone(),
            backdrop_path: details.backdrop_path.clone(),
            number_of_seasons: details.number_of_seasons,
        }
    }

    pub(super) fn head_line(&self) -> String {
        let mut parts = Vec::new();
        if let Some(year) = self.year {
            parts.push(year.to_string());
        }
        if let Some(country) = self.countries.first() {
            parts.push(country.clone());
        }
        parts.join(" - ")
    }
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
}

impl ReadyFiles {
    pub fn from_rows(hash: String, resume_id: Option<i32>, files: Vec<TorrentFileRow>) -> Self {
        Self {
            hash,
            files,
            resume_id,
            selected_id: resume_id,
        }
    }

    #[must_use]
    pub fn resume_index(&self) -> Option<usize> {
        let resume = self.resume_id?;
        self.files.iter().position(|file| file.id == resume)
    }
}

#[derive(Debug, Clone)]
pub enum FilesPane {
    Closed,
    Loading,
    Failed(String),
    Ready(ReadyFiles),
    Preloading { files: ReadyFiles, file_id: i32 },
}

impl FilesPane {
    #[must_use]
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::Closed)
    }

    #[must_use]
    pub fn is_spinning(&self) -> bool {
        matches!(self, Self::Loading | Self::Preloading { .. })
    }

    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    #[must_use]
    pub fn ready_or_preload(&self) -> Option<&ReadyFiles> {
        match self {
            Self::Ready(files) | Self::Preloading { files, .. } => Some(files),
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
    pub filters_open: bool,
    pub files: FilesPane,
    pub pick_gen: u64,
    pub pending_add: Option<AddSpec>,
}

impl TorrentState {
    pub fn from_details(details: &MediaDetails) -> Self {
        Self {
            kind: details.kind,
            id: details.id,
            movie: MovieBits::from_details(details),
            year: details.year,
            runtime_minutes: details.runtime_minutes,
            hits: TorrentHits::Loading,
            filter: TorrentFilter::default(),
            sort: SortMode::Popular,
            filters_open: false,
            files: FilesPane::Closed,
            pick_gen: 0,
            pending_add: None,
        }
    }

    pub fn matches(&self, kind: MediaKind, id: TmdbId) -> bool {
        self.kind == kind && self.id == id
    }
}

pub fn update(state: &mut TorrentState, event: Event, preferred: DefaultQuality) {
    match event {
        Event::ToggleFilters => state.filters_open = !state.filters_open,
        Event::FilterQuality(quality) => state.filter.quality = quality,
        Event::FilterHdr(choice) => state.filter.hdr = choice,
        Event::FilterDolby(choice) => state.filter.dolby = choice,
        Event::FilterSubs(choice) => state.filter.subs = choice,
        Event::FilterVoice(voice) => state.filter.voice = voice,
        Event::FilterLang(lang) => state.filter.lang = lang,
        Event::FilterSeason(season) => state.filter.season = season,
        Event::FilterYear(year) => state.filter.year = year,
        Event::FilterReset => state.filter = TorrentFilter::default(),
        Event::Sort(mode) => {
            state.sort = mode;
            if let TorrentHits::Ready(hits) = &mut state.hits {
                sort_hits(hits, state.kind, preferred, mode);
            }
        }
        Event::Pick(_)
        | Event::CloseFiles
        | Event::KeepOpen
        | Event::PickFile(_)
        | Event::RetryFiles => {}
    }
}
