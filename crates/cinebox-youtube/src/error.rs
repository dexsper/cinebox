//! Failures from YouTube extraction and the JS interpreter.

use std::fmt;

/// Library error. Messages are lowercase, no trailing punctuation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid youtube url")]
    InvalidUrl,
    #[error("youtube request failed")]
    Request(#[source] reqwest::Error),
    #[error("youtube returned HTTP {0}")]
    Http(u16),
    #[error("youtube returned unexpected json")]
    Json(#[from] serde_json::Error),
    #[error("video is unplayable")]
    Unplayable,
    #[error("no playable formats")]
    NoFormats,
    #[error("signature cipher is missing fields")]
    BadCipher,
    #[error("player javascript url is missing")]
    NoPlayer,
    #[error("could not decrypt signature")]
    BadSig,
    #[error("could not decrypt nsig")]
    BadNsig,
    #[error("{0}")]
    Js(JsError),
}

pub(crate) fn hide_url(err: reqwest::Error) -> reqwest::Error {
    err.without_url()
}

pub(crate) fn into_request(err: reqwest::Error) -> Error {
    Error::Request(hide_url(err))
}

/// JS interpreter failure or control-flow signal.
#[derive(Debug, Clone)]
pub enum JsError {
    Recursion,
    Break,
    Continue,
    Throw(String),
    Msg(String),
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Recursion => f.write_str("recursion limit reached"),
            Self::Break => f.write_str("invalid break"),
            Self::Continue => f.write_str("invalid continue"),
            Self::Throw(msg) => write!(f, "uncaught exception {msg}"),
            Self::Msg(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for JsError {}

impl From<JsError> for Error {
    fn from(err: JsError) -> Self {
        Error::Js(err)
    }
}

impl JsError {
    pub(crate) fn msg(msg: impl Into<String>) -> Self {
        Self::Msg(msg.into())
    }
}
