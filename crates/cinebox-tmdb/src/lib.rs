//! TMDB facade. Async reqwest `tmdb_client` 1.8.0 is blocking and not `Send`.

#![forbid(unsafe_code)]

mod catalog_map;
mod details;
mod details_dto;
mod details_map;
mod home;
mod seasons;

use std::time::Duration;

use cinebox_core::HomeCatalog;
use cinebox_net::NetConfig;
use serde::de::DeserializeOwned;

pub use home::{CatalogPage, MAX_ROW_ITEMS};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

const CONFIG_URL: &str = "https://api.themoviedb.org/3/configuration";
pub(crate) const USER_AGENT: &str = concat!("cinebox/", env!("CARGO_PKG_VERSION"));
pub(crate) const API_BASE: &str = "https://api.themoviedb.org/3";

/// Failures talking to TMDB. Never includes the API key.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("tmdb api key is empty")]
    EmptyKey,
    #[error("use the TMDB API key (32 hex), not the access token")]
    AccessToken,
    #[error("tmdb request failed: {}", request_reason(.0))]
    Request(#[source] reqwest::Error),
    #[error("tmdb api key was rejected")]
    Unauthorized,
    #[error("tmdb returned HTTP {0}")]
    Http(u16),
    #[error("media kind is not movie or tv")]
    UnsupportedKind,
    #[error("tmdb payload is incomplete")]
    IncompletePayload,
    #[error("tmdb returned unexpected json")]
    Json(#[from] serde_json::Error),
}

pub(crate) fn hide_url(err: reqwest::Error) -> reqwest::Error {
    err.without_url()
}

fn request_reason(err: &reqwest::Error) -> String {
    let mut out = err.to_string();
    let mut cause = std::error::Error::source(err);

    while let Some(err) = cause {
        out.push_str(": ");
        out.push_str(&err.to_string());
        cause = err.source();
    }

    out
}

pub(crate) fn into_request(err: reqwest::Error) -> Error {
    Error::Request(hide_url(err))
}

pub(crate) fn check_tmdb_status(status: reqwest::StatusCode) -> Result<(), Error> {
    if status.as_u16() == 401 {
        return Err(Error::Unauthorized);
    }
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }
    Ok(())
}

/// Send one TMDB request through the shared network layer (proxy first,
/// direct-DoH retry on transport failure). `build` may run twice.
pub(crate) async fn send<F>(net: &NetConfig, build: F) -> Result<reqwest::Response, Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    cinebox_net::send_resilient(net, CONNECT_TIMEOUT, Some(USER_AGENT), build)
        .await
        .map_err(into_request)
}

pub(crate) async fn send_json<T: DeserializeOwned, F>(net: &NetConfig, build: F) -> Result<T, Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    let response = send(net, build).await?;
    check_tmdb_status(response.status())?;

    response.json().await.map_err(into_request)
}

/// v3 `api_key` only. JWT access tokens (`eyJ…`) must not go in `?api_key=`.
pub(crate) fn prepare_api_key(api_key: &str) -> Result<&str, Error> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(Error::EmptyKey);
    }

    if key.starts_with("eyJ") {
        return Err(Error::AccessToken);
    }

    Ok(key)
}

/// `GET /3/configuration` with `api_key`. 401 means a bad key.
///
/// # Errors
///
/// Empty key, HTTP failures, or 401 from TMDB.
pub async fn check_api_key(api_key: &str, net: &NetConfig) -> Result<String, Error> {
    let api_key = prepare_api_key(api_key)?;

    let response = send(net, |client| {
        client
            .get(CONFIG_URL)
            .timeout(Duration::from_secs(15))
            .query(&[("api_key", api_key)])
    })
    .await?;

    check_tmdb_status(response.status())?;
    Ok(String::from("TMDB key ok"))
}

/// Fetch all remote home rows in parallel.
///
/// # Errors
///
/// Empty key or HTTP client build failure. Per-row HTTP errors are stored on the row.
pub async fn fetch_home(
    api_key: &str,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<HomeCatalog, Error> {
    home::fetch_home(api_key, language, net).await
}

/// One TMDB list page for a home shelf (`page` starts at 1).
///
/// Local history rows return an empty page. Remote rows follow TMDB pagination.
///
/// # Errors
///
/// Empty key, HTTP client build failure, or a TMDB HTTP/JSON error.
pub async fn fetch_catalog_page(
    api_key: &str,
    id: cinebox_core::HomeRowId,
    page: u32,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<CatalogPage, Error> {
    home::fetch_catalog_page(api_key, id, page, language, net).await
}

/// Movie/TV card: details, credits, videos, recs, similar, collection.
///
/// # Errors
///
/// Empty key, HTTP failures, or unexpected JSON.
pub async fn fetch_media(
    api_key: &str,
    kind: cinebox_core::MediaKind,
    id: cinebox_core::TmdbId,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<cinebox_core::MediaDetails, Error> {
    details::fetch_media(api_key, kind, id, language, net).await
}

/// Person bio + combined credits.
///
/// # Errors
///
/// Empty key, HTTP failures, or unexpected JSON.
pub async fn fetch_person(
    api_key: &str,
    id: cinebox_core::TmdbId,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<cinebox_core::PersonDetails, Error> {
    details::fetch_person(api_key, id, language, net).await
}

/// TV season episodes: names and `still_path` for the file-list preview.
///
/// # Errors
///
/// Empty key or HTTP/JSON failures. Unknown seasons are omitted.
pub async fn fetch_season_episodes(
    api_key: &str,
    tv_id: cinebox_core::TmdbId,
    seasons: &[u32],
    language: Option<&str>,
    net: &NetConfig,
) -> Result<Vec<seasons::SeasonEpisode>, Error> {
    seasons::fetch_season_episodes(api_key, tv_id, seasons, language, net).await
}

pub use seasons::SeasonEpisode;

/// Download a poster (or any image URL). Do not pass authenticated query strings.
///
/// # Errors
///
/// HTTP failures or a non-success status.
pub async fn download_image(url: &str, net: &NetConfig) -> Result<Vec<u8>, Error> {
    let response = send(net, |client| {
        client.get(url).timeout(Duration::from_secs(20))
    })
    .await?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }

    let bytes = response.bytes().await.map_err(into_request)?;
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_jwt_access_token() {
        assert!(matches!(prepare_api_key("  "), Err(Error::EmptyKey)));
        assert!(matches!(
            prepare_api_key("eyJhbGciOiJIUzI1NiJ9.e30.sig"),
            Err(Error::AccessToken)
        ));
        assert_eq!(prepare_api_key(" abc123 ").ok(), Some("abc123"));
    }

    #[tokio::test]
    #[ignore = "hits live TMDB"]
    async fn configuration_reaches_tmdb() {
        let net = NetConfig {
            use_system_proxy: true,
            ..NetConfig::direct()
        };
        let result = check_api_key("invalid-key", &net).await;
        assert!(
            matches!(result, Err(Error::Unauthorized | Error::Http(_))),
            "expected HTTP rejection, got {result:?}"
        );
    }
}
