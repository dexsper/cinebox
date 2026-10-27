//! Where TorrServer is and how to sign in to it.

use std::fmt;

use cinebox_core::normalize_base_url;

use super::error::Error;

/// A TorrServer address with its Basic auth; an empty username means none.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Server {
    pub url: String,
    pub username: String,
    pub password: String,
}

impl Server {
    pub(crate) fn base(&self) -> Result<String, Error> {
        normalize_base_url(&self.url).map_err(|_| Error::EmptyUrl)
    }

    pub(crate) fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.username.is_empty() {
            return request;
        }

        request.basic_auth(&self.username, Some(&self.password))
    }
}

impl fmt::Debug for Server {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Server")
            .field("url", &self.url)
            .field("username", &self.username)
            .field("password", &"redacted")
            .finish()
    }
}
