//! Jackett / Prowlarr connectivity and search.

#![forbid(unsafe_code)]

mod map;
mod query;
mod search;

use std::sync::OnceLock;
use std::time::Duration;

use cinebox_core::{ParserKind, join_url, normalize_base_url};
use serde::Deserialize;

pub use map::Hit;
pub use query::SearchQuery;
pub use search::search;

/// Failures talking to a parser. Never includes the API key.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("parser url is empty")]
    EmptyUrl,
    #[error("failed to build http client")]
    Client(#[source] reqwest::Error),
    #[error("parser request failed")]
    Request(#[source] reqwest::Error),
    #[error("parser returned HTTP {0}")]
    Http(u16),
    #[error("parser returned unexpected json")]
    BadJson(#[source] serde_json::Error),
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

fn apply_system_proxy(builder: reqwest::ClientBuilder, enabled: bool) -> reqwest::ClientBuilder {
    if enabled {
        return builder;
    }

    builder.no_proxy()
}

static CLIENTS: [OnceLock<reqwest::Client>; 2] = [OnceLock::new(), OnceLock::new()];

/// Shared long-lived client (one per proxy mode). Set request timeouts with
/// [`reqwest::RequestBuilder::timeout`].
pub(crate) fn http_client(use_system_proxy: bool) -> Result<reqwest::Client, Error> {
    let slot = &CLIENTS[usize::from(use_system_proxy)];

    if let Some(client) = slot.get() {
        return Ok(client.clone());
    }

    let client = apply_system_proxy(
        reqwest::Client::builder().connect_timeout(Duration::from_secs(8)),
        use_system_proxy,
    )
    .build()
    .map_err(Error::Client)?;

    Ok(slot.get_or_init(|| client).clone())
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
    use_system_proxy: bool,
) -> Result<String, Error> {
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    match kind {
        ParserKind::Jackett => ping_jackett(&base, api_key, use_system_proxy).await,
        ParserKind::Prowlarr => ping_prowlarr(&base, api_key, use_system_proxy).await,
    }
}

async fn ping_jackett(base: &str, api_key: &str, use_system_proxy: bool) -> Result<String, Error> {
    let url = join_url(base, "api/v2.0/indexers/all/results");
    let client = http_client(use_system_proxy)?;
    let response = client
        .get(&url)
        .timeout(PING_TIMEOUT)
        .query(&[("apikey", api_key), ("Query", "cinebox")])
        .send()
        .await
        .map_err(Error::Request)?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }

    let parsed: JackettResults = response.json().await.map_err(Error::Request)?;
    let n = parsed.results.as_ref().map_or(0, Vec::len);
    Ok(format!("Jackett ok ({n} results for test query)"))
}

async fn ping_prowlarr(base: &str, api_key: &str, use_system_proxy: bool) -> Result<String, Error> {
    let url = join_url(base, "api/v1/system/status");
    let client = http_client(use_system_proxy)?;
    let response = client
        .get(&url)
        .timeout(PING_TIMEOUT)
        .header("X-Api-Key", api_key)
        .query(&[("apikey", api_key)])
        .send()
        .await
        .map_err(Error::Request)?;

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
