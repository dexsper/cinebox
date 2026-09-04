//! Outgoing HTTP via `cinebox_net::send_resilient`.

use std::time::Duration;

use cinebox_net::NetConfig;
use serde::de::DeserializeOwned;

use crate::error::{into_request, Error};

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) const ANDROID_UA: &str =
    "com.google.android.youtube/20.10.38 (Linux; U; Android 11) gzip";
pub(crate) const ANDROID_CLIENT: &str = "ANDROID";
pub(crate) const ANDROID_VERSION: &str = "20.10.38";
pub(crate) const ANDROID_CLIENT_NAME: &str = "3";

pub(crate) async fn send<F>(net: &NetConfig, build: F) -> Result<reqwest::Response, Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    cinebox_net::send_resilient(net, CONNECT_TIMEOUT, Some(ANDROID_UA), build)
        .await
        .map_err(into_request)
}

pub(crate) async fn send_text<F>(net: &NetConfig, build: F) -> Result<String, Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    let response = send(net, build).await?;
    check_status(response.status())?;

    response.text().await.map_err(into_request)
}

pub(crate) async fn send_json<T: DeserializeOwned, F>(net: &NetConfig, build: F) -> Result<T, Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    let response = send(net, build).await?;
    check_status(response.status())?;

    response.json().await.map_err(into_request)
}

pub(crate) fn check_status(status: reqwest::StatusCode) -> Result<(), Error> {
    if status.is_success() {
        return Ok(());
    }

    Err(Error::Http(status.as_u16()))
}
