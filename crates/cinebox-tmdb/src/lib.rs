//! TMDB facade. Async reqwest — `tmdb_client` 1.8.0 is blocking and not `Send`.

#![forbid(unsafe_code)]

mod catalog_map;
mod details;
mod details_dto;
mod details_map;
mod home;

use std::time::Duration;

use cinebox_core::HomeCatalog;
use serde::de::DeserializeOwned;

pub use home::MAX_ROW_ITEMS;

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
    #[error("failed to build http client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("tmdb request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("tmdb api key was rejected")]
    Unauthorized,
    #[error("tmdb returned HTTP {0}")]
    Http(u16),
    #[error("tmdb returned unexpected json")]
    Json(#[from] serde_json::Error),
}

pub(crate) fn hide_url(err: reqwest::Error) -> reqwest::Error {
    err.without_url()
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

pub(crate) async fn send_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> Result<T, Error> {
    let response = request.send().await.map_err(into_request)?;
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

fn apply_system_proxy(builder: reqwest::ClientBuilder, enabled: bool) -> reqwest::ClientBuilder {
    if enabled { builder } else { builder.no_proxy() }
}

pub(crate) fn http_client(
    timeout: Duration,
    use_system_proxy: bool,
) -> Result<reqwest::Client, Error> {
    apply_system_proxy(
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(5)),
        use_system_proxy,
    )
    .build()
    .map_err(|err| Error::Client(hide_url(err)))
}

/// `GET /3/configuration` with `api_key`. 401 means a bad key.
///
/// # Errors
///
/// Empty key, HTTP failures, or 401 from TMDB.
pub async fn check_api_key(api_key: &str, use_system_proxy: bool) -> Result<String, Error> {
    let api_key = prepare_api_key(api_key)?;
    let client = http_client(Duration::from_secs(15), use_system_proxy)?;
    let response = client
        .get(CONFIG_URL)
        .query(&[("api_key", api_key)])
        .send()
        .await
        .map_err(into_request)?;
    check_tmdb_status(response.status())?;
    Ok(String::from("TMDB key ok"))
}

/// Fetch all remote home rows in parallel. Recently watched is always empty (Phase 8).
///
/// # Errors
///
/// Empty key or HTTP client build failure. Per-row HTTP errors are stored on the row.
pub async fn fetch_home(
    api_key: &str,
    language: Option<&str>,
    use_system_proxy: bool,
) -> Result<HomeCatalog, Error> {
    home::fetch_home(api_key, language, use_system_proxy).await
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
    use_system_proxy: bool,
) -> Result<cinebox_core::MediaDetails, Error> {
    details::fetch_media(api_key, kind, id, language, use_system_proxy).await
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
    use_system_proxy: bool,
) -> Result<cinebox_core::PersonDetails, Error> {
    details::fetch_person(api_key, id, language, use_system_proxy).await
}

/// Download a poster (or any image URL). Do not pass authenticated query strings.
///
/// # Errors
///
/// HTTP failures or a non-success status.
pub async fn download_image(url: &str, use_system_proxy: bool) -> Result<Vec<u8>, Error> {
    let client = http_client(Duration::from_secs(20), use_system_proxy)?;
    let response = client.get(url).send().await.map_err(into_request)?;
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
        let result = check_api_key("invalid-key", true).await;
        assert!(
            matches!(result, Err(Error::Unauthorized | Error::Http(_))),
            "expected HTTP rejection, got {result:?}"
        );
    }
}
