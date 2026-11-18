//! TMDB facade. Async reqwest `tmdb_client` 1.8.0 is blocking and not `Send`.

#![forbid(unsafe_code)]

mod catalog_map;
mod details;
mod details_dto;
mod details_map;
mod discover;
mod episode_groups;
mod genres;
mod home;
mod search;
mod seasons;
mod shelves;

use std::fmt;
use std::time::Duration;

use cinebox_core::{BaseUrlError, HomeCatalog, normalize_base_url, rehost_image_url};
use cinebox_net::NetConfig;
use serde::Deserialize;
use serde::de::DeserializeOwned;

pub use discover::{Date, DiscoverQuery, DiscoverSort};
pub use genres::{classify, company, genre, genres_for, keyword};
pub use home::{CatalogPage, MAX_ROW_ITEMS};
pub use search::{SearchKind, SearchQuery, TitleMatch};
pub use shelves::{SectionRow, ShelfId, ShelfRow, ShelfSource, section_rows};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// The official TMDB API, used when no other base is configured.
pub const OFFICIAL_API_BASE: &str = "https://api.themoviedb.org/3";

const OFFICIAL_ROOT: &str = "https://api.themoviedb.org";

const PROXY_SERVICE: &str = "cinebox-tmdb-proxy";

/// Failures talking to TMDB. Never includes the API key.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("tmdb api key is empty")]
    EmptyKey,
    #[error("use the TMDB API key (32 hex), not the access token")]
    AccessToken,
    #[error("tmdb proxy address is plain http to a remote host")]
    InsecureProxy,
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

/// A key that cannot be sent to TMDB. Converts into the matching [`Error`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    Empty,
    AccessToken,
    /// The address would carry the key in clear text; the caller decides which addresses are safe.
    InsecureProxy,
}

impl From<KeyError> for Error {
    fn from(error: KeyError) -> Self {
        match error {
            KeyError::Empty => Self::EmptyKey,
            KeyError::AccessToken => Self::AccessToken,
            KeyError::InsecureProxy => Self::InsecureProxy,
        }
    }
}

/// Where TMDB requests go and how they authorize: the official API or a `cinebox-tmdb-proxy`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Api {
    base: String,
    key: Option<String>,
}

impl fmt::Debug for Api {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = self.key.as_ref().map(|_| "***");

        formatter.debug_struct("Api").field("base", &self.base).field("key", &key).finish()
    }
}

impl Api {
    /// A blank `base` means [`OFFICIAL_API_BASE`]. `key` is `None` for a proxy that sends its own key.
    ///
    /// # Errors
    ///
    /// A present `key` that is blank or a JWT access token.
    pub fn new(base: &str, key: Option<&str>) -> Result<Self, KeyError> {
        let base = normalize_base_url(base).unwrap_or_else(|BaseUrlError::Empty| OFFICIAL_API_BASE.to_owned());
        let key = key.map(prepare_api_key).transpose()?.map(str::to_owned);

        Ok(Self { base, key })
    }

    /// Normalized base without a trailing slash, such as `https://api.themoviedb.org/3`.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The query pair that authorizes a request: `api_key`, or none for a keyless proxy.
    #[must_use]
    pub fn key_query(&self) -> Option<(&'static str, &str)> {
        self.key.as_deref().map(|key| ("api_key", key))
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base)
    }

    pub(crate) fn get(&self, client: &reqwest::Client, url: &str) -> reqwest::RequestBuilder {
        client.get(url).query(self.key_query().as_slice())
    }
}

/// What a `cinebox-tmdb-proxy` says about itself at `/info`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyInfo {
    pub version: String,

    /// The proxy forwards the viewer's key. `false` means it sends its own and the app needs none.
    pub key_required: bool,
}

#[derive(Deserialize)]
struct InfoBody {
    service: String,
    #[serde(default)]
    version: String,
    key_required: bool,
}

pub(crate) fn hide_url(error: reqwest::Error) -> reqwest::Error {
    error.without_url()
}

fn request_reason(error: &reqwest::Error) -> String {
    let mut out = error.to_string();
    let mut cause = std::error::Error::source(error);

    while let Some(source) = cause {
        out.push_str(": ");
        out.push_str(&source.to_string());
        cause = source.source();
    }

    out
}

pub(crate) fn into_request(error: reqwest::Error) -> Error {
    Error::Request(hide_url(error))
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
    cinebox_net::send_resilient(net, CONNECT_TIMEOUT, None, build).await.map_err(into_request)
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
pub(crate) fn prepare_api_key(api_key: &str) -> Result<&str, KeyError> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(KeyError::Empty);
    }

    if key.starts_with("eyJ") {
        return Err(KeyError::AccessToken);
    }

    Ok(key)
}

/// `GET /configuration` through `api`. 401 means a bad key; a keyless proxy is checked without one.
///
/// # Errors
///
/// HTTP failures, or 401 from TMDB.
pub async fn check_api_key(api: &Api, net: &NetConfig) -> Result<String, Error> {
    let url = api.url("configuration");
    let response = send(net, |client| api.get(client, &url).timeout(Duration::from_secs(15))).await?;

    check_tmdb_status(response.status())?;
    Ok(String::from("TMDB key ok"))
}

/// The normalized proxy root; `None` for a blank address or the official API.
fn proxy_root(proxy: &str) -> Option<String> {
    let normalized = normalize_base_url(proxy).ok()?;
    // A pasted API base such as `https://tmdb.example.com/3` still names the proxy root.
    let root = normalized.strip_suffix("/3").unwrap_or(&normalized).trim_end_matches('/');

    if root == OFFICIAL_ROOT {
        return None;
    }

    Some(root.to_owned())
}

/// The API base behind a proxy address: `{root}/3`, or [`OFFICIAL_API_BASE`] for a blank one.
#[must_use]
pub fn api_base(proxy: &str) -> String {
    let Some(root) = proxy_root(proxy) else { return OFFICIAL_API_BASE.to_owned() };

    format!("{root}/3")
}

/// The image base behind a proxy address: `{root}/t/p`, or empty for a blank one, which keeps the canonical
/// `image.tmdb.org` host.
#[must_use]
pub fn images_base(proxy: &str) -> String {
    proxy_root(proxy).map(|root| format!("{root}/t/p")).unwrap_or_default()
}

/// Ask the server at a proxy address (such as `https://tmdb.example.com`) whether it is a `cinebox-tmdb-proxy`.
///
/// `None` without a request for a blank address or the official API, and `None` when nothing answers or the
/// answer comes from another service.
pub async fn probe_proxy(proxy: &str, net: &NetConfig) -> Option<ProxyInfo> {
    let root = proxy_root(proxy)?;
    let url = format!("{root}/info");
    let response = send(net, |client| client.get(&url).timeout(PROBE_TIMEOUT)).await.ok()?;

    if !response.status().is_success() {
        return None;
    }

    let body: InfoBody = response.json().await.ok()?;
    if body.service != PROXY_SERVICE {
        return None;
    }

    Some(ProxyInfo { version: body.version, key_required: body.key_required })
}

/// Fetch all remote home rows in parallel. Per-row HTTP errors are stored on the row.
pub async fn fetch_home(api: &Api, language: Option<&str>, net: &NetConfig) -> HomeCatalog {
    home::fetch_home(api, language, net).await
}

/// One TMDB list page for a shelf (`page` starts at 1).
///
/// Local rows return an empty page. Remote rows follow TMDB pagination.
///
/// # Errors
///
/// HTTP client build failure, or a TMDB HTTP/JSON error.
pub async fn fetch_catalog_page(
    api: &Api,
    id: ShelfId,
    page: u32,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<CatalogPage, Error> {
    home::fetch_catalog_page(api, id, page, language, net).await
}

/// One `discover` page for user-chosen filters (`page` starts at 1).
///
/// # Errors
///
/// HTTP client build failure, or a TMDB HTTP/JSON error.
pub async fn fetch_discover_page(
    api: &Api,
    query: &DiscoverQuery,
    page: u32,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<CatalogPage, Error> {
    home::fetch_discover_page(api, query, page, language, net).await
}

/// First page of every shelf on a section hub, fetched in parallel.
///
/// Local rows come back empty. Per-shelf HTTP errors are stored on the row.
pub async fn fetch_section(
    api: &Api,
    section: cinebox_core::Section,
    language: Option<&str>,
    net: &NetConfig,
) -> Vec<ShelfRow> {
    shelves::fetch_section(net, api, language, section).await
}

/// One TMDB name-search page.
///
/// Empty queries return an empty page without calling TMDB.
///
/// # Errors
///
/// HTTP client build failure, or a TMDB HTTP/JSON error.
pub async fn fetch_search_page(
    api: &Api,
    search: SearchQuery<'_>,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<CatalogPage, Error> {
    search::fetch_search_page(api, search, language, net).await
}

/// First-page results of a movie or show name search, with their original titles, for telling which
/// title a release name means. A person search and an empty query find nothing without calling TMDB.
///
/// # Errors
///
/// HTTP client build failure, or a TMDB HTTP/JSON error.
pub async fn fetch_title_matches(
    api: &Api,
    search: SearchQuery<'_>,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<Vec<TitleMatch>, Error> {
    search::fetch_title_matches(api, search, language, net).await
}

/// Movie/TV card: details, credits, videos, recs, similar, collection.
///
/// # Errors
///
/// HTTP failures, or unexpected JSON.
pub async fn fetch_media(
    api: &Api,
    kind: cinebox_core::MediaKind,
    id: cinebox_core::TmdbId,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<cinebox_core::MediaDetails, Error> {
    details::fetch_media(api, kind, id, language, net).await
}

/// Person bio + combined credits.
///
/// # Errors
///
/// HTTP failures, or unexpected JSON.
pub async fn fetch_person(
    api: &Api,
    id: cinebox_core::TmdbId,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<cinebox_core::PersonDetails, Error> {
    details::fetch_person(api, id, language, net).await
}

/// TV season episodes: names and `still_path` for the file-list preview.
///
/// Seasons TMDB lacks are looked up through the show's Digital episode group,
/// so a release numbered by streaming "parts" still matches. Unknown seasons are omitted.
pub async fn fetch_season_episodes(
    api: &Api,
    tv_id: cinebox_core::TmdbId,
    seasons: &[u32],
    language: Option<&str>,
    net: &NetConfig,
) -> Vec<SeasonEpisode> {
    seasons::fetch_season_episodes(api, tv_id, seasons, language, net).await
}

pub use seasons::SeasonEpisode;

/// Download a poster (or any image URL). Do not pass authenticated query strings.
///
/// A canonical `https://image.tmdb.org/t/p/...` URL is fetched from `images_base` when that is not empty,
/// so a configured proxy is never bypassed.
///
/// # Errors
///
/// HTTP failures or a non-success status.
pub async fn download_image(url: &str, images_base: &str, net: &NetConfig) -> Result<Vec<u8>, Error> {
    let url = rehost_image_url(url, images_base);
    let response = send(net, |client| client.get(url.as_ref()).timeout(Duration::from_secs(20))).await?;

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
        assert_eq!(prepare_api_key("  "), Err(KeyError::Empty));
        assert_eq!(prepare_api_key("eyJhbGciOiJIUzI1NiJ9.e30.sig"), Err(KeyError::AccessToken));
        assert_eq!(prepare_api_key(" abc123 ").ok(), Some("abc123"));
    }

    #[tokio::test]
    #[ignore = "hits live TMDB"]
    async fn configuration_reaches_tmdb() {
        let net = NetConfig { use_system_proxy: true, ..NetConfig::direct() };
        let Ok(api) = Api::new("", Some("invalid-key")) else { panic!("a plain key is accepted") };
        let result = check_api_key(&api, &net).await;
        assert!(matches!(result, Err(Error::Unauthorized | Error::Http(_))), "expected HTTP rejection, got {result:?}");
    }
}
