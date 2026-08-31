//! Release-title parser, voice tags, bitrate, and torrent list filters.

#![forbid(unsafe_code)]

mod episode;
mod filter;
mod title;
mod voices;

pub use episode::{FileEpisode, file_display_name, parse_file_episode};

pub use cinebox_core::QualityBand;
pub use filter::{
    AudioLang, SortMode, TorrentFilter, TriChoice, VoiceFilter, VoiceKind, filtered_hits,
    matches_filter, season_options, sort_hits, voice_filter_options, year_options,
};
pub use title::{
    EpisodeSpan, Hdr, Resolution, SourceQuality, TitleInfo, format_bytes, infohash, parse_title,
};
pub use voices::{studios_in_catalog_order, voices};

/// Indexer row before title parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub title: String,
    pub tracker: String,
    pub size_bytes: u64,
    pub seeders: u32,
    pub peers: u32,
    pub magnet: String,
    pub published: String,
}

/// One indexer hit after title parse / bitrate / started matching.
#[derive(Debug, Clone, PartialEq)]
pub struct TorrentHit {
    pub title: String,
    pub tracker: String,
    pub size_bytes: u64,
    pub seeders: u32,
    pub peers: u32,
    pub magnet: String,
    pub published: String,
    pub info: TitleInfo,
    pub voices: Vec<&'static str>,
    pub bitrate_mbps: Option<f64>,
    pub started: bool,
}

impl TorrentHit {
    /// Parse a release name and attach tags.
    #[must_use]
    pub fn new(listing: Listing, runtime_minutes: Option<u32>, started_hashes: &[String]) -> Self {
        let info = parse_title(&listing.title);
        let found_voices = voices(&listing.title);
        let bitrate_mbps =
            runtime_minutes.and_then(|mins| estimate_bitrate_mbps(listing.size_bytes, mins));
        let started = infohash(&listing.magnet).is_some_and(|hash| {
            started_hashes
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&hash))
        });
        Self {
            title: listing.title,
            tracker: listing.tracker,
            size_bytes: listing.size_bytes,
            seeders: listing.seeders,
            peers: listing.peers,
            magnet: listing.magnet,
            published: listing.published,
            info,
            voices: found_voices,
            bitrate_mbps,
            started,
        }
    }

    /// Short size label (`1.8 GB`).
    #[must_use]
    pub fn size_label(&self) -> String {
        format_bytes(self.size_bytes)
    }
}

/// Mbps from byte size and runtime minutes: `(size * 8 / 1e6) / (minutes * 60)`.
#[must_use]
pub fn estimate_bitrate_mbps(size_bytes: u64, runtime_minutes: u32) -> Option<f64> {
    if size_bytes == 0 || runtime_minutes == 0 {
        return None;
    }
    let secs = f64::from(runtime_minutes) * 60.0;
    Some((size_bytes as f64 * 8.0 / 1_000_000.0) / secs)
}

/// Same estimate as [`estimate_bitrate_mbps`], using the stored hit size.
#[must_use]
pub fn hit_bitrate_mbps(hit: &TorrentHit, runtime_minutes: Option<u32>) -> Option<f64> {
    hit.bitrate_mbps
        .or_else(|| runtime_minutes.and_then(|mins| estimate_bitrate_mbps(hit.size_bytes, mins)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_matches_expected_formula() {
        let mbps = match estimate_bitrate_mbps(1_500_000_000, 120) {
            Some(value) => value,
            None => panic!("bitrate"),
        };
        assert!((mbps - 1.666).abs() < 0.01, "{mbps}");
    }

    #[test]
    fn started_matches_magnet_btih() {
        let hash = String::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let hit = TorrentHit::new(
            Listing {
                title: String::from("Dune.2021.1080p.WEB-DLRip"),
                tracker: String::from("rutracker"),
                size_bytes: 1_000,
                seeders: 10,
                peers: 1,
                magnet: format!("magnet:?xt=urn:btih:{hash}&dn=dune"),
                published: String::from("20 Aug 2021"),
            },
            Some(155),
            std::slice::from_ref(&hash),
        );
        assert!(hit.started);
        assert_eq!(hit.info.resolution, Some(Resolution::Fhd));
        assert_eq!(hit.info.quality, Some(SourceQuality::WebDlRip));
        let mbps = match hit.bitrate_mbps {
            Some(value) => value,
            None => panic!("bitrate should be set when size and runtime are known"),
        };
        assert!(mbps > 0.0, "{mbps}");
    }
}
