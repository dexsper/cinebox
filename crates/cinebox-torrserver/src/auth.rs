//! Basic auth header for mpv `http-header-fields`. Never log the return value.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// `Authorization: Basic …` for mpv. `None` when username is empty.
#[must_use]
pub fn mpv_http_header_fields(username: &str, password: &str) -> Option<String> {
    if username.is_empty() {
        return None;
    }
    let token = STANDARD.encode(format!("{username}:{password}"));
    Some(format!("Authorization: Basic {token}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_username_skips_header() {
        assert_eq!(mpv_http_header_fields("", "secret"), None);
    }

    #[test]
    fn encodes_user_pass() {
        let header = mpv_http_header_fields("user", "pass");
        assert_eq!(header.as_deref(), Some("Authorization: Basic dXNlcjpwYXNz"));
    }
}
