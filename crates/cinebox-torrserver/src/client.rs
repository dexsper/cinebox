//! Shared HTTP client: no proxy, optional Basic auth.

use std::time::Duration;

use reqwest::StatusCode;
use serde::de::DeserializeOwned;

use super::error::Error;

pub(crate) fn http_client(timeout: Duration) -> Result<reqwest::Client, Error> {
    // Local TorrServer must not inherit WinINet / env proxies.
    reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
        .build()
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
