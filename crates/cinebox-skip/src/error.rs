//! Error type for skip-segment providers.

/// Failures fetching segment data from an external provider.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("segment request failed")]
    Request(#[source] reqwest::Error),
    #[error("segment provider returned HTTP {0}")]
    Http(u16),
    #[error("segment provider returned unexpected JSON")]
    Json(#[from] serde_json::Error),
}
