//! Skip-segment data for Cinebox.

#![forbid(unsafe_code)]

mod error;
pub mod providers;

use std::time::Duration;

use cinebox_core::MediaKind;
use serde::{Deserialize, Serialize};

pub use error::Error;

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const USER_AGENT: &str = concat!("cinebox/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentType {
    Intro,
    Recap,
    Credits,
    Preview,
}

impl SegmentType {
    pub const ALL: [SegmentType; 4] = [
        SegmentType::Intro,
        SegmentType::Recap,
        SegmentType::Credits,
        SegmentType::Preview,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intro => "intro",
            Self::Recap => "recap",
            Self::Credits => "credits",
            Self::Preview => "preview",
        }
    }
}

/// Half-open time interval in milliseconds.
///
/// `None` start means from the beginning of the file;
/// `None` end means until the end of the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}

impl TimeRange {
    #[must_use]
    pub fn start_or_zero(&self) -> u64 {
        self.start_ms.unwrap_or(0)
    }

    #[must_use]
    pub fn end_or_duration(&self, duration_ms: u64) -> u64 {
        self.end_ms.unwrap_or(duration_ms)
    }

    #[must_use]
    pub fn contains(&self, position_ms: u64, duration_ms: u64) -> bool {
        let start = self.start_or_zero();
        let end = self.end_or_duration(duration_ms);
        position_ms >= start && position_ms < end
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaSegments {
    pub tmdb_id: u64,
    #[serde(rename = "type", default)]
    pub kind_str: String,
    #[serde(default)]
    pub intro: Vec<TimeRange>,
    #[serde(default)]
    pub recap: Vec<TimeRange>,
    #[serde(default)]
    pub credits: Vec<TimeRange>,
    #[serde(default)]
    pub preview: Vec<TimeRange>,
}

impl MediaSegments {
    #[must_use]
    pub fn segments_of(&self, ty: SegmentType) -> &[TimeRange] {
        match ty {
            SegmentType::Intro => &self.intro,
            SegmentType::Recap => &self.recap,
            SegmentType::Credits => &self.credits,
            SegmentType::Preview => &self.preview,
        }
    }
}

pub struct SegmentQuery {
    pub tmdb_id: u64,
    pub kind: MediaKind,
    /// `None` for movies.
    pub season: Option<u32>,
    /// `None` for movies.
    pub episode: Option<u32>,
    /// Actual file duration from the media player. The server uses this to
    /// reject mismatched encodes where the audio track has shifted.
    pub duration_ms: u64,
}

/// A source of skip-segment data.
///
/// `Ok(None)` means no data for this media (404 / duration mismatch).
#[allow(async_fn_in_trait)]
pub trait SegmentProvider {
    async fn fetch(
        &self,
        query: &SegmentQuery,
        net: &cinebox_net::NetConfig,
    ) -> Result<Option<MediaSegments>, Error>;
}

pub(crate) async fn send<F>(
    net: &cinebox_net::NetConfig,
    build: F,
) -> Result<reqwest::Response, Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    cinebox_net::send_resilient(net, CONNECT_TIMEOUT, Some(USER_AGENT), build)
        .await
        .map_err(|e| Error::Request(e.without_url()))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_range_contains_and_bounds() {
        let r = TimeRange { start_ms: Some(1000), end_ms: Some(5000) };

        assert!(r.contains(1000, 10_000));
        assert!(r.contains(4999, 10_000));
        assert!(!r.contains(5000, 10_000));
        assert!(!r.contains(999, 10_000));
    }

    #[test]
    fn time_range_null_start_is_zero() {
        let r = TimeRange { start_ms: None, end_ms: Some(23_000) };

        assert!(r.contains(0, 100_000));
        assert!(r.contains(22_999, 100_000));
        assert!(!r.contains(23_000, 100_000));
    }

    #[test]
    fn time_range_null_end_is_duration() {
        let r = TimeRange { start_ms: Some(5_801_777), end_ms: None };

        assert!(r.contains(5_801_777, 6_371_111));
        assert!(r.contains(6_371_110, 6_371_111));
        assert!(!r.contains(5_801_776, 6_371_111));
    }

    #[test]
    fn media_segments_json_roundtrip() {
        let json = r#"{
            "tmdb_id": 12345,
            "type": "movie",
            "intro": [{"start_ms": null, "end_ms": 23000}],
            "recap": [{"start_ms": 25000, "end_ms": 134000}],
            "credits": [
                {"start_ms": 5801777, "end_ms": 6371111},
                {"start_ms": 6408000, "end_ms": null}
            ],
            "preview": [{"start_ms": 1680000, "end_ms": 1740000}]
        }"#;

        let s: MediaSegments = serde_json::from_str(json).expect("parse");
        assert_eq!(s.tmdb_id, 12345);
        assert_eq!(s.intro.len(), 1);
        assert_eq!(s.intro[0].start_ms, None);
        assert_eq!(s.intro[0].end_ms, Some(23_000));
        assert_eq!(s.credits.len(), 2);
        assert_eq!(s.credits[1].start_ms, Some(6_408_000));
        assert_eq!(s.credits[1].end_ms, None);
    }

    #[test]
    fn segment_type_roundtrip() {
        let ty = SegmentType::Credits;
        let json = serde_json::to_string(&ty).unwrap();
        assert_eq!(json, r#""credits""#);

        let back: SegmentType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ty);
    }
}
