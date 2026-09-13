//! TheIntroDB HTTP provider.
//!
//! API: `GET https://api.theintrodb.org/v3/media`
//! Query params: `tmdb_id`, `duration_ms` (required), `season` + `episode`
//! (TV only).
//!
//! 200 → `MediaSegments`; 404 → `Ok(None)` (no data / duration mismatch);
//! other non-2xx → `Err(Error::Http(status))`.

use std::time::Duration;

use cinebox_core::MediaKind;
use cinebox_net::NetConfig;
use tracing::debug;

use crate::{Error, MediaSegments, SegmentProvider, SegmentQuery, send};

const API_URL: &str = "https://api.theintrodb.org/v3/media";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Provider backed by the TheIntroDB public HTTP API.
pub struct IntroDbProvider;

impl SegmentProvider for IntroDbProvider {
    async fn fetch(
        &self,
        query: &SegmentQuery,
        net: &NetConfig,
    ) -> Result<Option<MediaSegments>, Error> {
        let tmdb_id = query.tmdb_id.to_string();
        let duration_ms = query.duration_ms.to_string();

        let response = send(net, |client| {
            let mut req = client
                .get(API_URL)
                .timeout(REQUEST_TIMEOUT)
                .query(&[("tmdb_id", tmdb_id.as_str()), ("duration_ms", duration_ms.as_str())]);

            if query.kind == MediaKind::Tv {
                if let (Some(s), Some(e)) = (query.season, query.episode) {
                    let season = s.to_string();
                    let episode = e.to_string();
                    req = req.query(&[("season", season.as_str()), ("episode", episode.as_str())]);
                }
            }

            req
        })
        .await;

        let response = match response {
            Ok(r) => r,
            Err(e) => return Err(e),
        };

        let status = response.status();

        if status.as_u16() == 404 {
            debug!(tmdb_id = query.tmdb_id, "introdb: no segments (404)");
            return Ok(None);
        }

        if !status.is_success() {
            return Err(Error::Http(status.as_u16()));
        }

        let segments: MediaSegments = response
            .json()
            .await
            .map_err(|e| Error::Request(e.without_url()))?;

        debug!(
            tmdb_id = segments.tmdb_id,
            intro = segments.intro.len(),
            recap = segments.recap.len(),
            credits = segments.credits.len(),
            preview = segments.preview.len(),
            "introdb: segments fetched",
        );

        Ok(Some(segments))
    }
}

#[cfg(test)]
mod tests {
    use cinebox_net::NetConfig;
    use httpmock::prelude::*;

    use super::*;
    use crate::SegmentProvider;

    fn net() -> NetConfig {
        NetConfig::direct()
    }

    #[tokio::test]
    async fn movie_query_has_no_season_episode() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/v3/media")
                .query_param("tmdb_id", "12345")
                .query_param("duration_ms", "7200000")
                .query_param_missing("season")
                .query_param_missing("episode");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"tmdb_id":12345,"type":"movie","intro":[],"recap":[],"credits":[],"preview":[]}"#);
        });

        // Temporarily override API URL via a custom provider for tests
        let result = fetch_with_base(&server.base_url(), &SegmentQuery {
            tmdb_id: 12345,
            kind: MediaKind::Movie,
            season: None,
            episode: None,
            duration_ms: 7_200_000,
        }, &net()).await;

        mock.assert();
        assert!(result.unwrap().is_some());
    }

    #[tokio::test]
    async fn tv_query_includes_season_and_episode() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/v3/media")
                .query_param("tmdb_id", "67890")
                .query_param("duration_ms", "2700000")
                .query_param("season", "1")
                .query_param("episode", "1");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"tmdb_id":67890,"type":"tv","intro":[{"start_ms":null,"end_ms":23000}],"recap":[],"credits":[],"preview":[]}"#);
        });

        let result = fetch_with_base(&server.base_url(), &SegmentQuery {
            tmdb_id: 67890,
            kind: MediaKind::Tv,
            season: Some(1),
            episode: Some(1),
            duration_ms: 2_700_000,
        }, &net()).await;

        mock.assert();
        let segs = result.unwrap().unwrap();
        assert_eq!(segs.intro.len(), 1);
        assert_eq!(segs.intro[0].start_ms, None);
        assert_eq!(segs.intro[0].end_ms, Some(23_000));
    }

    #[tokio::test]
    async fn returns_none_on_404() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v3/media");
            then.status(404);
        });

        let result = fetch_with_base(&server.base_url(), &SegmentQuery {
            tmdb_id: 1,
            kind: MediaKind::Movie,
            season: None,
            episode: None,
            duration_ms: 100_000,
        }, &net()).await;

        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn returns_error_on_500() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v3/media");
            then.status(500);
        });

        let result = fetch_with_base(&server.base_url(), &SegmentQuery {
            tmdb_id: 1,
            kind: MediaKind::Movie,
            season: None,
            episode: None,
            duration_ms: 100_000,
        }, &net()).await;

        assert!(matches!(result, Err(Error::Http(500))));
    }

    /// Test helper: same logic as `IntroDbProvider::fetch` but against a
    /// custom base URL (mock server) instead of the production endpoint.
    async fn fetch_with_base(
        base: &str,
        query: &SegmentQuery,
        net: &NetConfig,
    ) -> Result<Option<MediaSegments>, Error> {
        let url = format!("{}/v3/media", base.trim_end_matches('/'));
        let tmdb_id = query.tmdb_id.to_string();
        let duration_ms = query.duration_ms.to_string();

        let response = crate::send(net, |client| {
            let mut req = client
                .get(&url)
                .timeout(REQUEST_TIMEOUT)
                .query(&[("tmdb_id", tmdb_id.as_str()), ("duration_ms", duration_ms.as_str())]);

            if query.kind == MediaKind::Tv {
                if let (Some(s), Some(e)) = (query.season, query.episode) {
                    let season = s.to_string();
                    let episode = e.to_string();
                    req = req.query(&[("season", season.as_str()), ("episode", episode.as_str())]);
                }
            }

            req
        })
        .await?;

        let status = response.status();

        if status.as_u16() == 404 {
            return Ok(None);
        }

        if !status.is_success() {
            return Err(Error::Http(status.as_u16()));
        }

        let segments: MediaSegments = response
            .json()
            .await
            .map_err(|e| Error::Request(e.without_url()))?;

        Ok(Some(segments))
    }
}
