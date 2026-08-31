//! Quality / voice / season filters and list ordering.

use std::fmt;

use cinebox_core::{MediaKind, QualityBand};

use crate::TorrentHit;
use crate::title::{Hdr, Resolution, TitleInfo, contains_any};
use crate::voices::studios_in_catalog_order;

#[must_use]
fn quality_matches(band: QualityBand, resolution: Option<Resolution>) -> bool {
    matches!(
        (band, resolution),
        (QualityBand::Uhd, Some(Resolution::Uhd))
            | (QualityBand::Fhd, Some(Resolution::Fhd))
            | (QualityBand::Hd, Some(Resolution::Hd))
            | (QualityBand::Sd, Some(Resolution::Sd))
    )
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceFilter {
    Dubbing,
    Polyphonic,
    TwoVoice,
    Amateur,
    Studio(&'static str),
}

impl VoiceFilter {
    pub const KINDS: &[Self] = &[
        Self::Dubbing,
        Self::Polyphonic,
        Self::TwoVoice,
        Self::Amateur,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioLang {
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

/// Active list filters. Empty / default shows everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TorrentFilter {
    pub quality: Vec<QualityBand>,
    pub hdr: TriChoice,
    pub dolby: TriChoice,
    pub subs: TriChoice,
    pub voice: Vec<VoiceFilter>,
    pub lang: Vec<AudioLang>,
    pub season: Vec<u32>,
    pub year: Vec<u16>,
}

impl TorrentFilter {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active_count() > 0
    }

    /// Unique filter categories that are not "Any".
    #[must_use]
    pub fn active_count(&self) -> usize {
        let mut n = 0;
        if !self.quality.is_empty() {
            n += 1;
        }
        if self.hdr != TriChoice::Any {
            n += 1;
        }
        if self.dolby != TriChoice::Any {
            n += 1;
        }
        if self.subs != TriChoice::Any {
            n += 1;
        }
        if !self.voice.is_empty() {
            n += 1;
        }
        if !self.lang.is_empty() {
            n += 1;
        }
        if !self.season.is_empty() {
            n += 1;
        }
        if !self.year.is_empty() {
            n += 1;
        }

        n
    }
}

/// Whether `hit` passes the active filter chips.
#[must_use]
pub fn matches_filter(hit: &TorrentHit, filter: &TorrentFilter) -> bool {
    if !filter.is_active() {
        return true;
    }

    let title = hit.title.to_lowercase();
    if !quality_ok(hit, filter) {
        return false;
    }

    if !tri_ok(filter.hdr, hit.info.hdr == Some(Hdr::Hdr)) {
        return false;
    }

    if !tri_ok(filter.dolby, hit.info.hdr == Some(Hdr::DolbyVision)) {
        return false;
    }

    if !tri_ok(filter.subs, has_subs(&title)) {
        return false;
    }

    if !voice_ok(hit, &filter.voice, &title) {
        return false;
    }

    if !lang_ok(&title, &filter.lang) {
        return false;
    }

    if !season_ok(hit, filter) {
        return false;
    }

    year_ok(hit, filter, &title)
}

fn quality_ok(hit: &TorrentHit, filter: &TorrentFilter) -> bool {
    if filter.quality.is_empty() {
        return true;
    }

    for band in &filter.quality {
        if quality_matches(*band, hit.info.resolution) {
            return true;
        }
    }

    false
}

fn season_ok(hit: &TorrentHit, filter: &TorrentFilter) -> bool {
    if filter.season.is_empty() {
        return true;
    }

    for season in &filter.season {
        if hit.info.seasons.contains(season) {
            return true;
        }
    }

    false
}

fn year_ok(hit: &TorrentHit, filter: &TorrentFilter, title: &str) -> bool {
    if filter.year.is_empty() {
        return true;
    }

    for year in &filter.year {
        if hit.info.year == Some(*year) {
            return true;
        }

        if title.contains(&year.to_string()) {
            return true;
        }
    }

    false
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

fn voice_ok(hit: &TorrentHit, filters: &[VoiceFilter], title: &str) -> bool {
    if filters.is_empty() {
        return true;
    }

    for filter in filters {
        if voice_one(hit, *filter, title) {
            return true;
        }
    }

    false
}

fn voice_one(hit: &TorrentHit, filter: VoiceFilter, title: &str) -> bool {
    match filter {
        VoiceFilter::Dubbing => voice_kind_ok(title, VoiceKind::Dubbing),
        VoiceFilter::Polyphonic => voice_kind_ok(title, VoiceKind::Polyphonic),
        VoiceFilter::TwoVoice => voice_kind_ok(title, VoiceKind::TwoVoice),
        VoiceFilter::Amateur => voice_kind_ok(title, VoiceKind::Amateur),
        VoiceFilter::Studio(name) => studio_ok(hit, name, title),
    }
}

fn studio_ok(hit: &TorrentHit, name: &str, title: &str) -> bool {
    for studio in &hit.voices {
        if studio.eq_ignore_ascii_case(name) {
            return true;
        }
    }

    title.contains(&name.to_lowercase())
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

fn lang_ok(title: &str, langs: &[AudioLang]) -> bool {
    if langs.is_empty() {
        return true;
    }

    for lang in langs {
        if lang_one(title, *lang) {
            return true;
        }
    }

    false
}

fn lang_one(title: &str, lang: AudioLang) -> bool {
    match lang {
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
pub fn filtered_hits<'a>(
    hits: &'a [TorrentHit],
    filter: &'a TorrentFilter,
) -> impl Iterator<Item = (usize, &'a TorrentHit)> {
    hits.iter()
        .enumerate()
        .filter(move |(_, hit)| matches_filter(hit, filter))
}

/// Voice-kind chips plus studios found in `hits`, then the selected studio if missing.
#[must_use]
pub fn voice_filter_options<'a>(
    hits: impl IntoIterator<Item = &'a TorrentHit>,
    selected: &[VoiceFilter],
) -> Vec<VoiceFilter> {
    let mut choices = VoiceFilter::KINDS.to_vec();
    let voices = hits.into_iter().flat_map(|hit| hit.voices.iter().copied());
    let studios = studios_in_catalog_order(voices);
   
    choices.extend(studios.into_iter().map(VoiceFilter::Studio));
    for filter in selected {
        let VoiceFilter::Studio(name) = filter else {
            continue;
        };
        
        if choices.contains(&VoiceFilter::Studio(name)) {
            continue;
        }

        choices.push(VoiceFilter::Studio(name));
    }

    choices
}

/// Card year, then up to 8 distinct hit years (newest first), then `selected` if missing.
#[must_use]
pub fn year_options(hits: &[TorrentHit], card_year: Option<u16>, selected: &[u16]) -> Vec<u16> {
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
    for year in selected {
        if years.contains(year) {
            continue;
        }

        years.push(*year);
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
pub fn sort_hits(hits: &mut [TorrentHit], kind: MediaKind, mode: SortMode) {
    hits.sort_by(|a, b| {
        b.started.cmp(&a.started).then_with(|| match mode {
            SortMode::Popular if kind == MediaKind::Tv => season_key(&b.info)
                .cmp(&season_key(&a.info))
                .then(episode_key(&b.info).cmp(&episode_key(&a.info)))
                .then(b.seeders.cmp(&a.seeders))
                .then(b.size_bytes.cmp(&a.size_bytes)),
            SortMode::Popular => b
                .seeders
                .cmp(&a.seeders)
                .then(b.size_bytes.cmp(&a.size_bytes)),
            SortMode::Seeders => b.seeders.cmp(&a.seeders),
            SortMode::Size => b.size_bytes.cmp(&a.size_bytes),
        })
    });
}

fn season_key(info: &TitleInfo) -> u32 {
    info.seasons.last().copied().unwrap_or(0)
}

/// Multi-season packs number episodes across the whole show, so their count
/// must not compete with the per-season numbering of a single-season release.
fn episode_key(info: &TitleInfo) -> u32 {
    if info.seasons.len() > 1 {
        return 0;
    }

    info.episodes.map(|span| span.to).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TorrentHit;

    fn hit(title: &str, seeders: u32, started: bool) -> TorrentHit {
        let mut row = hit_sized(title, seeders, 1_000);
        row.started = started;
        row
    }

    fn hit_sized(title: &str, seeders: u32, size_bytes: u64) -> TorrentHit {
        TorrentHit::new(
            crate::Listing {
                title: title.to_owned(),
                tracker: String::from("tr"),
                size_bytes,
                seeders,
                peers: 0,
                magnet: String::new(),
                published: String::new(),
            },
            None,
            &[],
        )
    }

    #[test]
    fn quality_and_hdr_filters() {
        let uhd = hit("Film.2160p.HDR.WEB-DL", 1, false);
        let fhd = hit("Film.1080p.WEB-DL", 1, false);
        assert!(matches_filter(
            &uhd,
            &TorrentFilter {
                quality: vec![QualityBand::Uhd],
                hdr: TriChoice::Yes,
                ..TorrentFilter::default()
            }
        ));
        assert!(!matches_filter(
            &fhd,
            &TorrentFilter {
                quality: vec![QualityBand::Uhd],
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
            &TorrentFilter {
                voice: vec![VoiceFilter::Studio("LostFilm")],
                ..TorrentFilter::default()
            }
        ));
        assert!(!matches_filter(
            &lost,
            &TorrentFilter {
                voice: vec![VoiceFilter::Studio("HDrezka")],
                ..TorrentFilter::default()
            }
        ));
    }

    #[test]
    fn started_sorts_first() {
        let mut hits = vec![hit("b", 100, false), hit("a", 1, true)];
        sort_hits(&mut hits, MediaKind::Movie, SortMode::Seeders);
        assert!(hits[0].started);
        assert!(!hits[1].started);
    }

    #[test]
    fn popular_tv_prefers_season_release_over_multi_season_pack() {
        let mut hits = vec![
            hit(
                "Show (1–3 сезон: 1–27 серии из 30) WEB-DL (1080p)",
                2,
                false,
            ),
            hit("Show [03x01-09 из 10] (2026) WEB-DLRip", 119, false),
        ];

        sort_hits(&mut hits, MediaKind::Tv, SortMode::Popular);

        assert_eq!(hits[0].seeders, 119);
    }

    #[test]
    fn popular_breaks_seeder_ties_by_size() {
        let mut hits = vec![
            hit_sized("Film.1080p.WEB-DL", 10, 4_000),
            hit_sized("Film.1080p.WEB-DL", 10, 8_000),
        ];

        sort_hits(&mut hits, MediaKind::Movie, SortMode::Popular);

        assert_eq!(hits[0].size_bytes, 8_000);
    }

    #[test]
    fn year_options_prefer_card_then_newest_hits() {
        let hits = vec![
            hit("A (2019)", 1, false),
            hit("B (2021)", 1, false),
            hit("C (2021)", 1, false),
        ];
        assert_eq!(year_options(&hits, Some(2020), &[]), vec![2020, 2021, 2019]);
        assert_eq!(
            year_options(&hits, Some(2020), &[2018]),
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

    #[test]
    fn quality_or_matches_any_selected_band() {
        let fhd = hit("Film.1080p.WEB-DL", 1, false);
        let filter = TorrentFilter {
            quality: vec![QualityBand::Uhd, QualityBand::Fhd],
            ..TorrentFilter::default()
        };

        assert!(matches_filter(&fhd, &filter));
        assert_eq!(filter.active_count(), 1);
    }

    #[test]
    fn active_count_is_unique_categories() {
        let filter = TorrentFilter {
            quality: vec![QualityBand::Uhd, QualityBand::Hd],
            hdr: TriChoice::Yes,
            voice: vec![VoiceFilter::Dubbing, VoiceFilter::Amateur],
            ..TorrentFilter::default()
        };

        assert_eq!(filter.active_count(), 3);
        assert!(filter.is_active());
        assert!(!TorrentFilter::default().is_active());
    }
}
