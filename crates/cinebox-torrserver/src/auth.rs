//! Basic auth header for the player's stream requests. Never log the return value.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// `Authorization: Basic …`. `None` when username is empty.
#[must_use]
pub fn basic_auth_header(username: &str, password: &str) -> Option<String> {
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
        assert_eq!(basic_auth_header("", "secret"), None);
    }

    #[test]
    fn encodes_user_pass() {
        let header = basic_auth_header("user", "pass");
        assert_eq!(header.as_deref(), Some("Authorization: Basic dXNlcjpwYXNz"));
    }
}
