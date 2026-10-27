//! TheIntroDB HTTP provider.
//!
//! API: `GET https://api.theintrodb.org/v4/media`
//! Query params: `tmdb_id`, `duration_ms`, `details=true`, and `season` +
//! `episode` for TV. Season and episode stay in TMDB numbering: v4 maps them
//! to TVDB itself when the lookup is by `tmdb_id`, so no `mapping` is sent.
//!
//! 200 gives `MediaSegments`, unless the matched version was timed for another
//! cut. 400 and 404 give `Ok(None)` (unknown media or episode), 429 gives
//! `Err(Error::RateLimited)`, other non-2xx give `Err(Error::Http(status))`.

use std::time::Duration;

use cinebox_core::MediaKind;
use cinebox_net::NetConfig;
use reqwest::StatusCode;
use serde::Deserialize;
use tracing::debug;

use crate::{Error, MediaSegments, SegmentProvider, SegmentQuery, send};

const BASE_URL: &str = "https://api.theintrodb.org/v4";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

// v4 picks the version closest to `duration_ms` within this window and falls
// back to the most-submitted one otherwise, so a wider gap is another cut.
const MAX_DURATION_DRIFT: Duration = Duration::from_mins(5);

/// Provider backed by the TheIntroDB public HTTP API.
#[derive(Debug, Clone)]
pub struct IntroDbProvider {
    base_url: String,
}

#[derive(Deserialize)]
struct MediaResponse {
    /// Length of the matched version; only sent with `details=true`.
    group_duration_ms: Option<u64>,

    #[serde(flatten)]
    segments: MediaSegments,
}

impl IntroDbProvider {
    /// `base_url` is the API root without `/media`, as in `https://api.theintrodb.org/v4`.
    #[must_use]
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

impl Default for IntroDbProvider {
    fn default() -> Self {
        Self::with_base_url(BASE_URL)
    }
}

impl SegmentProvider for IntroDbProvider {
    async fn fetch(
        &self,
        query: &SegmentQuery,
        net: &NetConfig,
    ) -> Result<Option<MediaSegments>, Error> {
        let Some(params) = query_params(query) else {
            return Ok(None);
        };

        let url = format!("{}/media", self.base_url.trim_end_matches('/'));
        let response = send(net, |client| {
            client.get(&url).timeout(REQUEST_TIMEOUT).query(&params)
        })
        .await?;
        let status = response.status();

        if status == StatusCode::NOT_FOUND || status == StatusCode::BAD_REQUEST {
            debug!(tmdb_id = query.tmdb_id, %status, "introdb: no segments");
            return Ok(None);
        }

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(Error::RateLimited);
        }

        if !status.is_success() {
            return Err(Error::Http(status.as_u16()));
        }

        let body: MediaResponse = response
            .json()
            .await
            .map_err(|error| Error::Request(error.without_url()))?;

        if !is_same_cut(body.group_duration_ms, query.duration_ms) {
            debug!(
                tmdb_id = query.tmdb_id,
                duration_ms = query.duration_ms,
                group_duration_ms = body.group_duration_ms,
                "introdb: segments timed for another cut",
            );
            return Ok(None);
        }

        let segments = body.segments;

        if segments.is_empty() {
            return Ok(None);
        }

        debug!(
            tmdb_id = query.tmdb_id,
            tvdb_id = segments.tvdb_id,
            intro = segments.intro.len(),
            recap = segments.recap.len(),
            credits = segments.credits.len(),
            preview = segments.preview.len(),
            "introdb: segments fetched",
        );

        Ok(Some(segments))
    }
}

/// `None` when the query cannot address a movie or an episode.
fn query_params(query: &SegmentQuery) -> Option<Vec<(&'static str, String)>> {
    let mut params = vec![
        ("tmdb_id", query.tmdb_id.to_string()),
        ("duration_ms", query.duration_ms.to_string()),
        ("details", String::from("true")),
    ];

    match query.kind {
        MediaKind::Movie => Some(params),
        MediaKind::Tv => {
            // Without both numbers v4 reads the id as a movie, and TMDB movie
            // and TV ids overlap, so it would answer for an unrelated film.
            let (Some(season), Some(episode)) = (query.season, query.episode) else {
                return None;
            };

            params.push(("season", season.to_string()));
            params.push(("episode", episode.to_string()));

            Some(params)
        }
        MediaKind::Person => None,
    }
}

// `None` and 0 mark a version of unknown length, which cannot be checked.
fn is_same_cut(group_duration_ms: Option<u64>, duration_ms: u64) -> bool {
    let Some(group_ms) = group_duration_ms.filter(|ms| *ms > 0) else {
        return true;
    };

    let drift = Duration::from_millis(group_ms.abs_diff(duration_ms));

    drift <= MAX_DURATION_DRIFT
}

#[cfg(test)]
mod tests {
    use cinebox_net::NetConfig;
    use httpmock::prelude::*;

    use super::*;

    const GOT_S01E01: &str = r#"{"tvdb_id":121361,"type":"tv","season":1,"episode":1,"group_duration_ms":3696273,"intro":[{"start_ms":435691,"end_ms":534851,"confidence":0.58,"submission_count":6}],"credits":[{"start_ms":3631088,"end_ms":3697669,"confidence":0.15,"submission_count":4}]}"#;

    fn provider(server: &MockServer) -> IntroDbProvider {
        IntroDbProvider::with_base_url(server.url("/v4"))
    }

    fn movie(tmdb_id: u64, duration_ms: u64) -> SegmentQuery {
        SegmentQuery {
            tmdb_id,
            kind: MediaKind::Movie,
            season: None,
            episode: None,
            duration_ms,
        }
    }

    fn episode(tmdb_id: u64, duration_ms: u64) -> SegmentQuery {
        SegmentQuery {
            tmdb_id,
            kind: MediaKind::Tv,
            season: Some(1),
            episode: Some(1),
            duration_ms,
        }
    }

    async fn fetch(
        server: &MockServer,
        query: &SegmentQuery,
    ) -> Result<Option<MediaSegments>, Error> {
        provider(server).fetch(query, &NetConfig::direct()).await
    }

    #[tokio::test]
    async fn movie_query_has_no_season_episode() -> Result<(), Error> {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/v4/media")
                .query_param("tmdb_id", "12345")
                .query_param("duration_ms", "7200000")
                .query_param("details", "true")
                .query_param_missing("season")
                .query_param_missing("episode")
                .query_param_missing("mapping");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{"tvdb_id":777,"type":"movie","intro":[{"start_ms":null,"end_ms":23000}]}"#,
                );
        });

        let result = fetch(&server, &movie(12345, 7_200_000)).await;

        mock.assert();
        assert!(result?.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn tv_query_includes_season_and_episode() -> Result<(), Error> {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/v4/media")
                .query_param("tmdb_id", "1399")
                .query_param("duration_ms", "3696000")
                .query_param("season", "1")
                .query_param("episode", "1")
                .query_param_missing("mapping");
            then.status(200)
                .header("content-type", "application/json")
                .body(GOT_S01E01);
        });

        let result = fetch(&server, &episode(1399, 3_696_000)).await;

        mock.assert();
        let Some(segments) = result? else {
            panic!("no segments for a known episode")
        };
        assert_eq!(segments.tvdb_id, Some(121_361));
        assert_eq!(segments.intro.len(), 1);
        assert_eq!(segments.intro[0].start_ms, Some(435_691));
        assert_eq!(segments.credits[0].end_ms, Some(3_697_669));
        assert!(segments.recap.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn tv_query_without_episode_is_not_sent() -> Result<(), Error> {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/v4/media");
            then.status(200)
                .header("content-type", "application/json")
                .body(GOT_S01E01);
        });
        let query = SegmentQuery {
            episode: None,
            ..episode(1399, 3_696_000)
        };

        let result = fetch(&server, &query).await;

        mock.assert_calls(0);
        assert!(result?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn rejects_version_timed_for_another_cut() -> Result<(), Error> {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v4/media");
            then.status(200)
                .header("content-type", "application/json")
                .body(GOT_S01E01);
        });

        let result = fetch(&server, &episode(1399, 1_000_000)).await;

        assert!(result?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn accepts_version_of_unknown_length() -> Result<(), Error> {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v4/media");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"tvdb_id":1,"type":"movie","group_duration_ms":0,"credits":[{"start_ms":100,"end_ms":null}]}"#);
        });

        let result = fetch(&server, &movie(1, 100_000_000)).await;

        assert!(result?.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn returns_none_without_segments() -> Result<(), Error> {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v4/media");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"tvdb_id":1,"type":"tv","episodes":[]}"#);
        });

        let result = fetch(&server, &episode(1, 100_000)).await;

        assert!(result?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn returns_none_on_404() -> Result<(), Error> {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v4/media");
            then.status(404)
                .body(r#"{"error":"no submissions for this media","code":"no_submissions"}"#);
        });

        let result = fetch(&server, &movie(1, 100_000)).await;

        assert!(result?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn returns_none_on_400() -> Result<(), Error> {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v4/media");
            then.status(400)
                .body(r#"{"error":"episode not found on TVDB"}"#);
        });

        let result = fetch(&server, &episode(1, 100_000)).await;

        assert!(result?.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn returns_rate_limited_on_429() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v4/media");
            then.status(429).body(r#"{"error":"Too Many Requests"}"#);
        });

        let result = fetch(&server, &movie(1, 100_000)).await;

        assert!(matches!(result, Err(Error::RateLimited)));
    }

    #[tokio::test]
    async fn returns_error_on_500() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v4/media");
            then.status(500);
        });

        let result = fetch(&server, &movie(1, 100_000)).await;

        assert!(matches!(result, Err(Error::Http(500))));
    }

    // One test for all live lookups: `cinebox-net` caches clients process-wide,
    // and a pooled connection dies with the runtime of the test that opened it.
    #[tokio::test]
    #[ignore = "hits live TheIntroDB"]
    async fn live_lookups() -> Result<(), Error> {
        let provider = IntroDbProvider::default();
        let net = NetConfig::direct();
        let unknown_episode = SegmentQuery {
            season: Some(99),
            ..episode(1399, 3_696_273)
        };

        let got_s01e01 = provider.fetch(&episode(1399, 3_696_273), &net).await?;
        let matrix = provider.fetch(&movie(603, 8_180_662), &net).await?;
        let another_cut = provider.fetch(&episode(1399, 1_000_000), &net).await?;
        let unknown = provider.fetch(&unknown_episode, &net).await?;

        let Some(got_s01e01) = got_s01e01 else {
            panic!("Game of Thrones S01E01 has no segments")
        };

        assert_eq!(got_s01e01.tvdb_id, Some(121_361));
        assert!(!got_s01e01.intro.is_empty());
        assert!(matrix.is_some_and(|segments| !segments.intro.is_empty()));
        assert!(another_cut.is_none());
        assert!(unknown.is_none());

        Ok(())
    }
}
