//! Port of the release-name decoder (`general()`).

use std::sync::LazyLock;

use regex::Regex;

/// Structured fields extracted from a torrent title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleInfo {
    /// Seasons covered. Defaults to `[1]` when the title has none.
    pub seasons: Vec<u32>,
    pub episodes: Option<EpisodeSpan>,
    pub year: Option<u16>,
    pub quality: Option<SourceQuality>,
    pub hdr: Option<Hdr>,
    pub resolution: Option<Resolution>,
}

impl Default for TitleInfo {
    fn default() -> Self {
        Self {
            seasons: vec![1],
            episodes: None,
            year: None,
            quality: None,
            hdr: None,
            resolution: None,
        }
    }
}

impl TitleInfo {
    /// `"1"` or `"1-2"` for UI tags.
    #[must_use]
    pub fn season_label(&self) -> String {
        match self.seasons.as_slice() {
            [] => String::from("1"),
            [one] => one.to_string(),
            [first, .., last] => format!("{first}-{last}"),
        }
    }
}

/// Inclusive episode range shown on a row (`1-7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpisodeSpan {
    pub from: u32,
    pub to: u32,
}

/// Source encode, longest token first so `WEB-DLRip` wins over `WEB-DL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceQuality {
    WebDlRip,
    WebDl,
    WebRip,
    Web,
    BluRay,
    BdRip,
    HdRip,
    Hdtv,
    DvdRip,
    Dvd,
    Cam,
    Ts,
}

impl SourceQuality {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WebDlRip => "WEB-DLRip",
            Self::WebDl => "WEB-DL",
            Self::WebRip => "WEBRip",
            Self::Web => "WEB",
            Self::BluRay => "BluRay",
            Self::BdRip => "BDRip",
            Self::HdRip => "HDRip",
            Self::Hdtv => "HDTV",
            Self::DvdRip => "DVDRip",
            Self::Dvd => "DVD",
            Self::Cam => "CAM",
            Self::Ts => "TS",
        }
    }
}

/// HDR flavour in the release name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hdr {
    Hdr,
    DolbyVision,
}

/// Parsed resolution band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Uhd,
    Qhd,
    Fhd,
    Hd,
    Sd,
    Ld,
}

impl Resolution {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uhd => "4K",
            Self::Qhd => "2K",
            Self::Fhd => "FHD",
            Self::Hd => "HD",
            Self::Sd => "SD",
            Self::Ld => "LD",
        }
    }

    /// Higher is sharper. Used to rank against the player default.
    #[must_use]
    pub const fn rank(self) -> i32 {
        match self {
            Self::Uhd => 3,
            Self::Qhd | Self::Fhd => 2,
            Self::Hd => 1,
            Self::Sd | Self::Ld => 0,
        }
    }
}

/// Decode season / episodes / year / quality / HDR / resolution from a release name.
#[must_use]
pub fn parse_title(title: &str) -> TitleInfo {
    if title.is_empty() {
        return TitleInfo::default();
    }

    parse_title_lower(title, &title.to_lowercase())
}

/// [`parse_title`] for a title the caller has already lowercased.
pub(crate) fn parse_title_lower(title: &str, lower: &str) -> TitleInfo {
    if title.is_empty() {
        return TitleInfo::default();
    }

    TitleInfo {
        seasons: seasons(title),
        episodes: episodes(title),
        year: year(title),
        quality: source_quality(lower),
        hdr: hdr(lower),
        resolution: resolution(lower),
    }
}

fn seasons(title: &str) -> Vec<u32> {
    static RANGE: LazyLock<[Regex; 3]> = LazyLock::new(|| {
        [
            re(r"\[S(\d{1,2})[-–](\d{1,2})\]"),
            re(r"(?i)(?:сезон|season)\s*(\d{1,2})[-–](\d{1,2})"),
            re(r"(?i)(\d{1,2})[-–](\d{1,2})\s*(?:сезон|season)"),
        ]
    });
    static SINGLE: LazyLock<[Regex; 7]> = LazyLock::new(|| {
        [
            re(r"(?i)(\d+)\s*(?:сезон|season)"),
            re(r"(?i)(?:сезон|season):?\s*(\d{1,2})"),
            re(r"(?i)(\d{1,2})(?:st|nd|rd|th)?\s+season"),
            re(r"(?i)(?:тв|tv)-(\d+)"),
            re(r"(?i)\b(\d{1,2})x\d{1,2}"),
            re(r"(?i)\bs(\d{1,2})e"),
            re(r"(?i)\bs(\d{1,2})\b"),
        ]
    });

    if let Some((from, to)) = first_pair(RANGE.as_slice(), title) {
        let lo = from.min(to);
        let hi = from.max(to);
        return (lo..=hi).collect();
    }
    match first_one(SINGLE.as_slice(), title) {
        Some(n) => vec![n],
        None => vec![1],
    }
}

fn episodes(title: &str) -> Option<EpisodeSpan> {
    static NX: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\d{1,2}x(\d{1,2})[-–](\d{1,2})"));
    static RANGE: LazyLock<[Regex; 8]> = LazyLock::new(|| {
        [
            re(r"(?i)(\d+)\s*[-–]\s*(\d+)\s*(?:серия|episode)"),
            re(r"(?i)(\d{1,3})[-–](\d{1,3})\s*серии"),
            re(r"(?i)[\[(](\d{1,3})\s*[-–](\d{1,3})\s*(?:из|з|of)\s*(\d{1,3})"),
            re(r"(?i)[\[(](\d{1,3})\s*[-–]\s*(?:из|з|of)\s*(\d{1,3})"),
            re(r"(?i)[\[(](\d{1,3})\s*(?:из|з|of)\s*(\d{1,3})[\])]"),
            re(r"(?i)e(\d{1,3})\s*[-–]\s*(?:(?:из|з|of)\s+)?(\d{1,3})"),
            re(r"(?i)(?:серии|серія|episodes)\s*(\d{1,3})\s*(?:(?:из|з|of)\s+)?(\d{1,3})"),
            re(r"(?i)(?:серии|episodes):\s*(\d+)[-–](\d+)"),
        ]
    });
    static SINGLE: LazyLock<[Regex; 6]> = LazyLock::new(|| {
        [
            re(r"(?i)\bs\d{1,2}e(\d+)"),
            re(r"(?i)\d{1,2}x(\d{1,2})\b"),
            re(r"(?i)\be(\d{1,2})\b"),
            re(r"(?i)[\[(](\d{1,2})\s+(?:из|з|of)\s+\d{1,2}[\])]"),
            re(r"(?i)(\d{1,2})\s+(?:из|з|of)\s+\d{1,2}"),
            re(r"(?i)(\d+)\s*(?:серия|episode)"),
        ]
    });

    if let Some((from, to)) = pair(&NX, title) {
        return Some(EpisodeSpan { from, to });
    }
    if let Some((a, b)) = first_pair(RANGE.as_slice(), title)
        && !(a >= 1900 && b >= 1900)
    {
        return Some(span_from_range(a, b));
    }
    first_one(SINGLE.as_slice(), title).map(|to| EpisodeSpan { from: 1, to })
}

fn span_from_range(a: u32, b: u32) -> EpisodeSpan {
    if a == b {
        EpisodeSpan { from: 1, to: b }
    } else if a > 1 {
        EpisodeSpan { from: 1, to: a }
    } else {
        EpisodeSpan { from: a, to: b }
    }
}

fn year(title: &str) -> Option<u16> {
    static RE: LazyLock<[Regex; 3]> = LazyLock::new(|| {
        [
            re(r"\(((?:19|20)\d{2})[-–](?:19|20)\d{2}\)"),
            re(r"\(((?:19|20)\d{2})\)"),
            re(r"\b((?:19|20)\d{2})\b"),
        ]
    });
    first_one(RE.as_slice(), title).and_then(|y| u16::try_from(y).ok())
}

/// Longest token first; a match must not continue with a letter (`WEB-DL` ≠ `WEB-DLRip`).
/// `lower` is the release name already lowercased.
fn source_quality(lower: &str) -> Option<SourceQuality> {
    const TOKENS: &[(SourceQuality, &[&str])] = &[
        (SourceQuality::WebDlRip, &["web-dlrip", "webdlrip"]),
        (SourceQuality::WebDl, &["web-dl", "webdl"]),
        (SourceQuality::WebRip, &["webrip"]),
        (SourceQuality::Web, &["web"]),
        (SourceQuality::BluRay, &["blu-ray", "bluray"]),
        (SourceQuality::BdRip, &["bdrip"]),
        (SourceQuality::HdRip, &["hdrip"]),
        (SourceQuality::Hdtv, &["hdtv"]),
        (SourceQuality::DvdRip, &["dvdrip"]),
        (SourceQuality::Dvd, &["dvd"]),
        (SourceQuality::Cam, &["cam"]),
        (SourceQuality::Ts, &["ts"]),
    ];
    TOKENS
        .iter()
        .find(|(_, needles)| needles.iter().any(|needle| token_at(lower, needle)))
        .map(|(quality, _)| *quality)
}

fn hdr(lower: &str) -> Option<Hdr> {
    if lower.contains("dolby vision") {
        return Some(Hdr::DolbyVision);
    }

    if lower.contains("hdr") {
        return Some(Hdr::Hdr);
    }

    None
}

/// `lower` is the release name already lowercased.
fn resolution(lower: &str) -> Option<Resolution> {
    const TOKENS: &[(Resolution, &[&str])] = &[
        (Resolution::Uhd, &["2160p", "2160р", "4k", "uhd", "ultrahd"]),
        (Resolution::Qhd, &["1440p"]),
        (Resolution::Fhd, &["1080p", "1080р", "fullhd"]),
        (Resolution::Hd, &["720p", "720р"]),
        (Resolution::Sd, &["480p"]),
        (Resolution::Ld, &["360p"]),
    ];

    TOKENS
        .iter()
        .find(|(_, needles)| contains_any(lower, needles))
        .map(|(band, _)| *band)
}

pub(crate) fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn token_at(haystack: &str, needle: &str) -> bool {
    let Some(pos) = haystack.find(needle) else {
        return false;
    };
    let rest = &haystack[pos + needle.len()..];
    !rest.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
}

/// IEC-ish size label (1024-based), same spirit as the JS helper.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let n = bytes as f64;
    if n >= KIB * KIB * KIB {
        return format!("{:.1} GB", n / (KIB * KIB * KIB));
    }

    if n >= KIB * KIB {
        return format!("{:.0} MB", n / (KIB * KIB));
    }

    if n >= KIB {
        return format!("{:.0} KB", n / KIB);
    }

    format!("{bytes} B")
}

/// Infohash from a magnet (`btih:`) when present.
#[must_use]
pub fn infohash(link: &str) -> Option<String> {
    let lower = link.to_ascii_lowercase();
    let rest = lower.split("btih:").nth(1)?;
    let hash: String = rest
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    (hash.len() == 40 || hash.len() == 32).then_some(hash)
}

fn re(pattern: &'static str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("title regex {pattern}: {error}"))
}

fn first_one(patterns: &[Regex], title: &str) -> Option<u32> {
    patterns.iter().find_map(|re| one(re, title))
}

fn first_pair(patterns: &[Regex], title: &str) -> Option<(u32, u32)> {
    patterns.iter().find_map(|re| pair(re, title))
}

fn one(re: &Regex, title: &str) -> Option<u32> {
    re.captures(title)?.get(1)?.as_str().parse().ok()
}

fn pair(re: &Regex, title: &str) -> Option<(u32, u32)> {
    let caps = re.captures(title)?;
    Some((
        caps.get(1)?.as_str().parse().ok()?,
        caps.get(2)?.as_str().parse().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seasons_episodes_year_quality() {
        let a = parse_title("Show 4 сезон 04x01-07 из 10 1080p WEB-DLRip");
        assert_eq!(a.seasons, vec![4]);
        assert_eq!(a.episodes, Some(EpisodeSpan { from: 1, to: 7 }));
        assert_eq!(a.resolution, Some(Resolution::Fhd));
        assert_eq!(a.quality, Some(SourceQuality::WebDlRip));

        let b = parse_title("Show [S01-02] (2022-2023) 2160p HDR");
        assert_eq!(b.seasons, vec![1, 2]);
        assert_eq!(b.season_label(), "1-2");
        assert_eq!(b.year, Some(2022));
        assert_eq!(b.resolution, Some(Resolution::Uhd));
        assert_eq!(b.hdr, Some(Hdr::Hdr));

        let c = parse_title("Movie (2024) WEB-DL 720p");
        assert_eq!(c.seasons, vec![1]);
        assert_eq!(c.year, Some(2024));
        assert_eq!(c.quality, Some(SourceQuality::WebDl));
        assert_eq!(c.resolution, Some(Resolution::Hd));
    }

    #[test]
    fn web_dl_does_not_steal_web_dlrip() {
        let info = parse_title("Film WEB-DLRip");
        assert_eq!(info.quality, Some(SourceQuality::WebDlRip));
    }

    #[test]
    fn size_and_hash() {
        assert_eq!(format_bytes(1024), "1 KB");
        assert_eq!(
            infohash("magnet:?xt=urn:btih:ABCDEF0123456789ABCDEF0123456789ABCDEF01&dn=x")
                .as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef01")
        );
        assert_eq!(infohash("http://example/file.torrent"), None);
    }
}
