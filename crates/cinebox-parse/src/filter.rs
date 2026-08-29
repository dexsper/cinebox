//! Quality / voice / season filters and list ordering.

use std::fmt;

use cinebox_core::{DefaultQuality, MediaKind};

use crate::TorrentHit;
use crate::title::{Hdr, Resolution, TitleInfo, contains_any};
use crate::voices::studios_in_catalog_order;

/// Resolution band used by the torrent list chips (4K / 1080p / 720p).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityBand {
    Uhd,
    Fhd,
    Hd,
}

impl QualityBand {
    pub const ALL: &[Self] = &[Self::Uhd, Self::Fhd, Self::Hd];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Uhd => "4K",
            Self::Fhd => "1080p",
            Self::Hd => "720p",
        }
    }

    #[must_use]
    fn matches(self, resolution: Option<Resolution>) -> bool {
        matches!(
            (self, resolution),
            (Self::Uhd, Some(Resolution::Uhd))
                | (Self::Fhd, Some(Resolution::Fhd))
                | (Self::Hd, Some(Resolution::Hd))
        )
    }
}

/// Voice-type chips (studio names live on the hit via [`crate::voices`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VoiceKind {
    #[default]
    Any,
    Dubbing,
    Polyphonic,
    TwoVoice,
    Amateur,
}

impl VoiceKind {
    pub const ALL: &[Self] = &[
        Self::Dubbing,
        Self::Polyphonic,
        Self::TwoVoice,
        Self::Amateur,
    ];

    pub const PICK: &[Self] = &[
        Self::Any,
        Self::Dubbing,
        Self::Polyphonic,
        Self::TwoVoice,
        Self::Amateur,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::Dubbing => "Dubbing",
            Self::Polyphonic => "Polyphonic",
            Self::TwoVoice => "Two-voiced",
            Self::Amateur => "Amateur",
        }
    }
}

impl fmt::Display for VoiceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Translation menu: voice kind, then studio names found in the results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VoiceFilter {
    #[default]
    Any,
    Dubbing,
    Polyphonic,
    TwoVoice,
    Amateur,
    Studio(&'static str),
}

impl VoiceFilter {
    pub const KINDS: &[Self] = &[
        Self::Any,
        Self::Dubbing,
        Self::Polyphonic,
        Self::TwoVoice,
        Self::Amateur,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::Dubbing => "Dubbing",
            Self::Polyphonic => "Polyphonic",
            Self::TwoVoice => "Two-voiced",
            Self::Amateur => "Amateur",
            Self::Studio(name) => name,
        }
    }
}

impl fmt::Display for VoiceFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Yes / no / don't care, as in the HDR / DV / subs menus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TriChoice {
    #[default]
    Any,
    Yes,
    No,
}

impl TriChoice {
    pub const ALL: &[Self] = &[Self::Any, Self::Yes, Self::No];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::Yes => "Yes",
            Self::No => "No",
        }
    }
}

impl fmt::Display for TriChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Audio/subtitle language token in the release name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AudioLang {
    #[default]
    Any,
    Ru,
    En,
    Uk,
    Ja,
    Ko,
    Zh,
    De,
    Fr,
}

impl AudioLang {
    pub const ALL: &[Self] = &[
        Self::Any,
        Self::Ru,
        Self::En,
        Self::Uk,
        Self::Ja,
        Self::Ko,
        Self::Zh,
        Self::De,
        Self::Fr,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::Ru => "Russian",
            Self::En => "English",
            Self::Uk => "Ukrainian",
            Self::Ja => "Japanese",
            Self::Ko => "Korean",
            Self::Zh => "Chinese",
            Self::De => "German",
            Self::Fr => "French",
        }
    }
}

impl fmt::Display for AudioLang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// How the visible list is ordered (started hits stay first).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortMode {
    #[default]
    Popular,
    Seeders,
    Size,
}

impl SortMode {
    pub const ALL: &[Self] = &[Self::Popular, Self::Seeders, Self::Size];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Popular => "Popular",
            Self::Seeders => "Seeders",
            Self::Size => "Size",
        }
    }
}

impl fmt::Display for SortMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl fmt::Display for QualityBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Active list filters. Empty / default shows everything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TorrentFilter {
    pub quality: Option<QualityBand>,
    pub hdr: TriChoice,
    pub dolby: TriChoice,
    pub subs: TriChoice,
    pub voice: VoiceFilter,
    pub lang: AudioLang,
    pub season: Option<u32>,
    pub year: Option<u16>,
}

impl TorrentFilter {
    #[must_use]
    pub fn is_active(self) -> bool {
        self.quality.is_some()
            || self.hdr != TriChoice::Any
            || self.dolby != TriChoice::Any
            || self.subs != TriChoice::Any
            || self.voice != VoiceFilter::Any
            || self.lang != AudioLang::Any
            || self.season.is_some()
            || self.year.is_some()
    }
}

/// Whether `hit` passes the active filter chips.
#[must_use]
pub fn matches_filter(hit: &TorrentHit, filter: TorrentFilter) -> bool {
    if !filter.is_active() {
        return true;
    }
    let title = hit.title.to_lowercase();
    filter
        .quality
        .is_none_or(|band| band.matches(hit.info.resolution))
        && tri_ok(filter.hdr, hit.info.hdr == Some(Hdr::Hdr))
        && tri_ok(filter.dolby, hit.info.hdr == Some(Hdr::DolbyVision))
        && tri_ok(filter.subs, has_subs(&title))
        && voice_ok(hit, filter.voice, &title)
        && lang_ok(&title, filter.lang)
        && filter
            .season
            .is_none_or(|season| hit.info.seasons.contains(&season))
        && filter
            .year
            .is_none_or(|year| hit.info.year == Some(year) || title.contains(&year.to_string()))
}

fn tri_ok(choice: TriChoice, present: bool) -> bool {
    match choice {
        TriChoice::Any => true,
        TriChoice::Yes => present,
        TriChoice::No => !present,
    }
}

fn has_subs(title: &str) -> bool {
    title.contains(" sub") || has_token(title, "ст")
}

fn voice_ok(hit: &TorrentHit, filter: VoiceFilter, title: &str) -> bool {
    match filter {
        VoiceFilter::Any => true,
        VoiceFilter::Dubbing => voice_kind_ok(title, VoiceKind::Dubbing),
        VoiceFilter::Polyphonic => voice_kind_ok(title, VoiceKind::Polyphonic),
        VoiceFilter::TwoVoice => voice_kind_ok(title, VoiceKind::TwoVoice),
        VoiceFilter::Amateur => voice_kind_ok(title, VoiceKind::Amateur),
        VoiceFilter::Studio(name) => {
            hit.voices
                .iter()
                .any(|studio| studio.eq_ignore_ascii_case(name))
                || title.contains(&name.to_lowercase())
        }
    }
}

fn voice_kind_ok(title: &str, kind: VoiceKind) -> bool {
    match kind {
        VoiceKind::Any => true,
        VoiceKind::Dubbing => {
            contains_any(title, &["дублирован", "дубляж", "apple"])
                || has_token(title, "dub")
                || has_token(title, "дб")
        }
        VoiceKind::Polyphonic => {
            title.contains("многоголос") || has_token(title, "лм") || has_token(title, "пм")
        }
        VoiceKind::TwoVoice => {
            contains_any(title, &["двухголос", "двуголос"])
                || has_token(title, "l2")
                || has_token(title, "лд")
                || has_token(title, "пд")
        }
        VoiceKind::Amateur => {
            contains_any(title, &["любитель", "авторский"])
                || has_token(title, "l1")
                || has_token(title, "ло")
                || has_token(title, "ап")
        }
    }
}

fn has_token(title: &str, token: &str) -> bool {
    title
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == token)
}

fn lang_ok(title: &str, lang: AudioLang) -> bool {
    match lang {
        AudioLang::Any => true,
        AudioLang::Ru => {
            has_token(title, "ru") || has_token(title, "rus") || title.contains("russian")
        }
        AudioLang::En => {
            has_token(title, "en") || has_token(title, "eng") || title.contains("english")
        }
        AudioLang::Uk => has_token(title, "uk") || has_token(title, "ua") || title.contains("ukr"),
        AudioLang::Ja => has_token(title, "ja") || has_token(title, "jp") || title.contains("jap"),
        AudioLang::Ko => has_token(title, "ko") || has_token(title, "kr") || title.contains("kor"),
        AudioLang::Zh => has_token(title, "zh") || has_token(title, "cn") || title.contains("chi"),
        AudioLang::De => has_token(title, "de") || title.contains("german"),
        AudioLang::Fr => has_token(title, "fr") || title.contains("french"),
    }
}

/// Hits that pass `filter`, with original list indices.
pub fn filtered_hits(
    hits: &[TorrentHit],
    filter: TorrentFilter,
) -> impl Iterator<Item = (usize, &TorrentHit)> {
    hits.iter()
        .enumerate()
        .filter(move |(_, hit)| matches_filter(hit, filter))
}

/// Voice-kind chips plus studios found in `hits`, then the selected studio if missing.
#[must_use]
pub fn voice_filter_options<'a>(
    hits: impl IntoIterator<Item = &'a TorrentHit>,
    selected: VoiceFilter,
) -> Vec<VoiceFilter> {
    let mut choices = VoiceFilter::KINDS.to_vec();
    let studios =
        studios_in_catalog_order(hits.into_iter().flat_map(|hit| hit.voices.iter().copied()));
    choices.extend(studios.into_iter().map(VoiceFilter::Studio));
    if let VoiceFilter::Studio(name) = selected
        && !choices.contains(&VoiceFilter::Studio(name))
    {
        choices.push(VoiceFilter::Studio(name));
    }
    choices
}

/// Card year, then up to 8 distinct hit years (newest first), then `selected` if missing.
#[must_use]
pub fn year_options(
    hits: &[TorrentHit],
    card_year: Option<u16>,
    selected: Option<u16>,
) -> Vec<u16> {
    let mut years = Vec::new();
    if let Some(year) = card_year {
        years.push(year);
    }
    let mut extra: Vec<u16> = hits.iter().filter_map(|hit| hit.info.year).collect();
    extra.sort_unstable();
    extra.reverse();
    extra.dedup();
    for year in extra.into_iter().take(8) {
        if !years.contains(&year) {
            years.push(year);
        }
    }
    if let Some(year) = selected
        && !years.contains(&year)
    {
        years.push(year);
    }
    years
}

/// Distinct seasons from hits, sorted ascending.
#[must_use]
pub fn season_options(hits: &[TorrentHit]) -> Vec<u32> {
    let mut seasons: Vec<u32> = hits
        .iter()
        .flat_map(|hit| hit.info.seasons.iter().copied())
        .collect();
    seasons.sort_unstable();
    seasons.dedup();
    seasons
}

/// Sort in place. Started torrents stay at the top.
pub fn sort_hits(
    hits: &mut [TorrentHit],
    kind: MediaKind,
    preferred: DefaultQuality,
    mode: SortMode,
) {
    hits.sort_by(|a, b| {
        b.started.cmp(&a.started).then_with(|| match mode {
            SortMode::Popular if kind == MediaKind::Tv => season_key(&b.info)
                .cmp(&season_key(&a.info))
                .then(episode_key(&b.info).cmp(&episode_key(&a.info)))
                .then(b.seeders.cmp(&a.seeders)),
            SortMode::Popular => b
                .quality_score(preferred)
                .cmp(&a.quality_score(preferred))
                .then(b.seeders.cmp(&a.seeders)),
            SortMode::Seeders => b.seeders.cmp(&a.seeders),
            SortMode::Size => b.size_bytes.cmp(&a.size_bytes),
        })
    });
}

fn season_key(info: &TitleInfo) -> u32 {
    info.seasons.last().copied().unwrap_or(0)
}

fn episode_key(info: &TitleInfo) -> u32 {
    info.episodes.map(|span| span.to).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TorrentHit;

    fn hit(title: &str, seeders: u32, started: bool) -> TorrentHit {
        let mut row = TorrentHit::new(
            crate::Listing {
                title: title.to_owned(),
                tracker: String::from("tr"),
                size_bytes: 1_000,
                seeders,
                peers: 0,
                magnet: String::new(),
                published: String::new(),
            },
            None,
            &[],
        );
        row.started = started;
        row
    }

    #[test]
    fn quality_and_hdr_filters() {
        let uhd = hit("Film.2160p.HDR.WEB-DL", 1, false);
        let fhd = hit("Film.1080p.WEB-DL", 1, false);
        assert!(matches_filter(
            &uhd,
            TorrentFilter {
                quality: Some(QualityBand::Uhd),
                hdr: TriChoice::Yes,
                ..TorrentFilter::default()
            }
        ));
        assert!(!matches_filter(
            &fhd,
            TorrentFilter {
                quality: Some(QualityBand::Uhd),
                ..TorrentFilter::default()
            }
        ));
    }

    #[test]
    fn translation_studio_matches_detected_voice() {
        let lost = hit("Film.1080p.WEB-DL.LostFilm", 1, false);
        assert!(lost.voices.contains(&"LostFilm"));
        assert!(matches_filter(
            &lost,
            TorrentFilter {
                voice: VoiceFilter::Studio("LostFilm"),
                ..TorrentFilter::default()
            }
        ));
        assert!(!matches_filter(
            &lost,
            TorrentFilter {
                voice: VoiceFilter::Studio("HDrezka"),
                ..TorrentFilter::default()
            }
        ));
    }

    #[test]
    fn started_sorts_first() {
        let mut hits = vec![hit("b", 100, false), hit("a", 1, true)];
        sort_hits(
            &mut hits,
            MediaKind::Movie,
            DefaultQuality::Q1080p,
            SortMode::Seeders,
        );
        assert!(hits[0].started);
        assert!(!hits[1].started);
    }

    #[test]
    fn year_options_prefer_card_then_newest_hits() {
        let hits = vec![
            hit("A (2019)", 1, false),
            hit("B (2021)", 1, false),
            hit("C (2021)", 1, false),
        ];
        assert_eq!(
            year_options(&hits, Some(2020), None),
            vec![2020, 2021, 2019]
        );
        assert_eq!(
            year_options(&hits, Some(2020), Some(2018)),
            vec![2020, 2021, 2019, 2018]
        );
    }

    #[test]
    fn season_options_are_sorted_unique() {
        let hits = vec![
            hit("Show.S02", 1, false),
            hit("Show.S01", 1, false),
            hit("Show.S02", 1, false),
        ];
        assert_eq!(season_options(&hits), vec![1, 2]);
    }
}
