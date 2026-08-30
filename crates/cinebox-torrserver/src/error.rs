//! Failures talking to TorrServer. Never includes passwords.

/// HTTP / JSON failures. Display and logs must not contain credentials.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("torrserver url is empty")]
    EmptyUrl,
    #[error("torrent link is empty")]
    EmptyLink,
    #[error("torrent hash is empty")]
    EmptyHash,
    #[error("failed to build http client")]
    Client(#[source] reqwest::Error),
    #[error("torrserver request failed")]
    Request(#[source] reqwest::Error),
    #[error("torrserver returned HTTP {0}")]
    Http(u16),
    #[error("torrent not found")]
    NotFound,
    #[error("torrserver returned empty echo")]
    EmptyEcho,
    #[error("speed test received no data")]
    NoData,
    #[error("torrent files did not appear in time")]
    FilesTimeout,
    #[error("preload did not finish in time")]
    PreloadTimeout,
    #[error("torrserver returned unexpected json")]
    BadJson(#[source] serde_json::Error),
}
