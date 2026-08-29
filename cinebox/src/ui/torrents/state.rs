//! Torrent explorer state and filter events.

use cinebox_core::{DefaultQuality, MediaDetails, MediaKind, TmdbId};
use cinebox_parse::{
    AudioLang, QualityBand, SortMode, TorrentFilter, TorrentHit, TriChoice, VoiceFilter, sort_hits,
};

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
}

#[derive(Debug, Clone)]
pub struct MovieBits {
    pub title: String,
    pub overview: Option<String>,
    pub year: Option<u16>,
    pub vote: Option<f32>,
    pub genres: Vec<String>,
    pub countries: Vec<String>,
    pub poster_path: Option<String>,
}

impl MovieBits {
    pub fn from_details(details: &MediaDetails) -> Self {
        Self {
            title: details.title.clone(),
            overview: details.overview.clone(),
            year: details.year,
            vote: details.vote,
            genres: details.genres.iter().take(3).cloned().collect(),
            countries: details.countries.clone(),
            poster_path: details.poster_path.clone(),
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
    pub pick_hint: bool,
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
            pick_hint: false,
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
        Event::Pick(_) => state.pick_hint = true,
    }
}
