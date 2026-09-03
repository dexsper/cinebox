//! Shared HTTP client: no proxy, optional Basic auth.

use std::time::Duration;

use cinebox_net::NetConfig;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;

use super::error::Error;

/// Shared long-lived client from the common factory. TorrServer is local, so
/// the config is forced direct: no WinINet / env proxies and no DoH. Set
/// request timeouts with [`reqwest::RequestBuilder::timeout`].
pub(crate) fn http_client() -> Result<reqwest::Client, Error> {
    cinebox_net::plain_client(&NetConfig::direct(), Duration::from_secs(5), None)
        .map_err(Error::Client)
}

pub(crate) fn apply_basic_auth(
    request: reqwest::RequestBuilder,
    username: &str,
    password: &str,
) -> reqwest::RequestBuilder {
    if username.is_empty() {
        return request;
    }
    request.basic_auth(username, Some(password))
}

pub(crate) fn check_status(status: StatusCode) -> Result<(), Error> {
    if status == StatusCode::NOT_FOUND {
        return Err(Error::NotFound);
    }
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }
    Ok(())
}

pub(crate) async fn send_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> Result<T, Error> {
    let response = request.send().await.map_err(Error::Request)?;
    check_status(response.status())?;
    let body = response.bytes().await.map_err(Error::Request)?;
    serde_json::from_slice(&body).map_err(Error::BadJson)
}
