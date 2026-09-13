//! Jackett / Prowlarr connectivity, search, and hit ranking.

#![forbid(unsafe_code)]

mod filter;
mod map;
mod query;
mod search;
mod title;
mod voices;

use std::time::Duration;

use cinebox_core::{ParserKind, join_url, normalize_base_url, typograph};
use cinebox_net::NetConfig;
use serde::Deserialize;

pub use filter::{
    AudioLang, SortMode, TorrentFilter, TriChoice, VoiceFilter, VoiceKind, filtered_hits,
    matches_filter, season_options, sort_hits, voice_filter_options, year_options,
};
pub use map::Hit;
pub use query::SearchQuery;
pub use search::search;
pub use title::{
    EpisodeSpan, Hdr, Resolution, SourceQuality, TitleInfo, format_bytes, infohash, parse_title,
};
pub use voices::{studios_in_catalog_order, voices};

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Failures talking to a parser. Never includes the API key.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("parser url is empty")]
    EmptyUrl,
    #[error("parser request failed")]
    Request(#[source] reqwest::Error),
    #[error("parser returned HTTP {0}")]
    Http(u16),
    #[error("parser returned unexpected json")]
    BadJson(#[source] serde_json::Error),
}

/// Send one parser request through the shared network layer (proxy first,
/// direct-DoH retry on transport failure). `build` may run twice.
pub(crate) async fn send<F>(net: &NetConfig, build: F) -> Result<reqwest::Response, Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    cinebox_net::send_resilient(net, CONNECT_TIMEOUT, None, build)
        .await
        .map_err(Error::Request)
}

#[derive(Debug, Deserialize)]
struct JackettResults {
    #[serde(rename = "Results")]
    results: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ProwlarrStatus {
    version: Option<String>,
}

const PING_TIMEOUT: Duration = Duration::from_secs(20);

/// Lightweight parser ping (Jackett results endpoint / Prowlarr system status).
///
/// # Errors
///
/// Empty URL, HTTP failures, or JSON that does not match the expected shape.
pub async fn ping(
    kind: ParserKind,
    base_url: &str,
    api_key: &str,
    net: &NetConfig,
) -> Result<String, Error> {
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    match kind {
        ParserKind::Jackett => ping_jackett(&base, api_key, net).await,
        ParserKind::Prowlarr => ping_prowlarr(&base, api_key, net).await,
    }
}

async fn ping_jackett(base: &str, api_key: &str, net: &NetConfig) -> Result<String, Error> {
    let url = join_url(base, "api/v2.0/indexers/all/results");
    let response = send(net, |client| {
        client
            .get(&url)
            .timeout(PING_TIMEOUT)
            .query(&[("apikey", api_key), ("Query", "cinebox")])
    })
    .await?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }

    let parsed: JackettResults = response.json().await.map_err(Error::Request)?;
    let n = parsed.results.as_ref().map_or(0, Vec::len);
    Ok(format!("Jackett ok ({n} results for test query)"))
}

async fn ping_prowlarr(base: &str, api_key: &str, net: &NetConfig) -> Result<String, Error> {
    let url = join_url(base, "api/v1/system/status");
    let response = send(net, |client| {
        client
            .get(&url)
            .timeout(PING_TIMEOUT)
            .header("X-Api-Key", api_key)
            .query(&[("apikey", api_key)])
    })
    .await?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }

    let body = response.bytes().await.map_err(Error::Request)?;
    let parsed: ProwlarrStatus = serde_json::from_slice(&body).map_err(Error::BadJson)?;

    Ok(match parsed.version {
        Some(version) if !version.is_empty() => format!("Prowlarr ok (v{version})"),
        _ => String::from("Prowlarr ok"),
    })
}

/// One [`Hit`] after title parse / bitrate / started matching.
#[derive(Debug, Clone, PartialEq)]
pub struct TorrentHit {
    pub title: String,
    pub title_lower: String,
    pub display_title: String,
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
    pub local_rank: Option<u8>,
}

impl TorrentHit {
    /// Parse a release name and attach tags.
    #[must_use]
    pub fn new(
        hit: Hit,
        runtime_minutes: Option<u32>,
        started_hashes: &[String],
        local_hashes: &[String],
    ) -> Self {
        let size_bytes = hit.size_bytes;
        let title_lower = hit.title.to_lowercase();
        let title_display = typograph(&hit.title);
        let info_from_title = title::parse_title_lower(&hit.title, &title_lower);

        let found_voices = voices::voices_lower(&title_lower);
        let bitrate_mbps = runtime_minutes.and_then(|m| estimate_bitrate_mbps(size_bytes, m));

        let hash = infohash(&hit.magnet);
        let local_rank = local_rank_of(&hit.magnet, local_hashes);

        let started = hash.as_ref().is_some_and(|hash| {
            started_hashes
                .iter()
                .any(|known| known.eq_ignore_ascii_case(hash))
        });

        Self {
            title: hit.title,
            title_lower,
            display_title: title_display,
            tracker: hit.tracker,
            size_bytes: hit.size_bytes,
            seeders: hit.seeders,
            peers: hit.peers,
            magnet: hit.magnet,
            published: hit.published,
            info: info_from_title,
            voices: found_voices,
            bitrate_mbps,
            started,
            local_rank,
        }
    }

    /// Refresh local recency from play history. Never clears an existing tag.
    pub fn mark_local(&mut self, local_hashes: &[String]) {
        let Some(rank) = local_rank_of(&self.magnet, local_hashes) else {
            return;
        };

        self.local_rank = Some(rank);
    }

    /// Short size label (`1.8 GB`).
    #[must_use]
    pub fn size_label(&self) -> String {
        format_bytes(self.size_bytes)
    }
}

fn local_rank_of(magnet: &str, local_hashes: &[String]) -> Option<u8> {
    let hash = infohash(magnet)?;
    let rank = local_hashes
        .iter()
        .position(|known| known.eq_ignore_ascii_case(&hash))?;

    u8::try_from(rank).ok()
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
    if let Some(bitrate) = hit.bitrate_mbps {
        return Some(bitrate);
    }

    runtime_minutes.and_then(|mins| estimate_bitrate_mbps(hit.size_bytes, mins))
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
            Hit {
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
            &[],
        );

        assert!(hit.started);
        assert_eq!(hit.local_rank, None);
        assert_eq!(hit.info.resolution, Some(Resolution::Fhd));
        assert_eq!(hit.info.quality, Some(SourceQuality::WebDlRip));

        let mbps = match hit.bitrate_mbps {
            Some(value) => value,
            None => panic!("bitrate should be set when size and runtime are known"),
        };

        assert!(mbps > 0.0, "{mbps}");
    }

    #[test]
    fn local_matches_magnet_btih() {
        let hash = String::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        let hit = TorrentHit::new(
            Hit {
                title: String::from("Dune.2021.1080p.WEB-DLRip"),
                tracker: String::from("rutracker"),
                size_bytes: 1_000,
                seeders: 10,
                peers: 1,
                magnet: format!("magnet:?xt=urn:btih:{hash}&dn=dune"),
                published: String::new(),
            },
            None,
            &[],
            std::slice::from_ref(&hash),
        );

        assert!(!hit.started);
        assert_eq!(hit.local_rank, Some(0));
    }

    #[test]
    fn mark_local_promotes_without_clearing_started() {
        let hash = String::from("cccccccccccccccccccccccccccccccccccccccc");
        let mut hit = TorrentHit::new(
            Hit {
                title: String::from("Dune.2021.1080p.WEB-DLRip"),
                tracker: String::from("rutracker"),
                size_bytes: 1_000,
                seeders: 10,
                peers: 1,
                magnet: format!("magnet:?xt=urn:btih:{hash}&dn=dune"),
                published: String::new(),
            },
            None,
            std::slice::from_ref(&hash),
            &[],
        );

        assert!(hit.started);
        assert_eq!(hit.local_rank, None);

        hit.mark_local(&[hash]);

        assert!(hit.started);
        assert_eq!(hit.local_rank, Some(0));
    }

    #[test]
    fn mark_local_refreshes_rank_on_rewatch() {
        let older = String::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let newest = String::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        let mut hit = TorrentHit::new(
            Hit {
                title: String::from("Dune.2021.1080p.WEB-DLRip"),
                tracker: String::from("rutracker"),
                size_bytes: 1_000,
                seeders: 1,
                peers: 0,
                magnet: format!("magnet:?xt=urn:btih:{newest}&dn=dune"),
                published: String::new(),
            },
            None,
            &[],
            &[older.clone(), newest.clone()],
        );

        assert_eq!(hit.local_rank, Some(1));
        hit.mark_local(&[newest, older]);

        assert_eq!(hit.local_rank, Some(0));
    }

    #[test]
    fn display_title_decodes_entities_and_keeps_raw() {
        let hit = TorrentHit::new(
            Hit {
                title: String::from("DoMiNo &amp; селезень &quot;Silo&quot;"),
                tracker: String::from("rutracker"),
                size_bytes: 1_000,
                seeders: 1,
                peers: 0,
                magnet: String::new(),
                published: String::new(),
            },
            None,
            &[],
            &[],
        );

        assert_eq!(hit.title, "DoMiNo &amp; селезень &quot;Silo&quot;");
        assert!(
            !hit.display_title.contains("&amp;"),
            "{:?}",
            hit.display_title
        );

        assert!(
            !hit.display_title.contains("&quot;"),
            "{:?}",
            hit.display_title
        );

        assert!(
            hit.display_title.contains("DoMiNo & "),
            "{:?}",
            hit.display_title
        );

        assert!(
            hit.display_title.contains('«') || hit.display_title.contains('\u{201C}'),
            "{:?}",
            hit.display_title
        );
    }
}
