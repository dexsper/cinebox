//! Normalization and format checks applied when a settings field commits.

/// How a committed text field is cleaned up before it reaches [`cinebox_core::Settings`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// Kept byte for byte (passwords).
    Raw,
    Plain,
    Url,
    Key,
}

/// Problem with a TMDB key that is visible without a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyHint {
    /// A v4 read access token (`eyJ…`) pasted where the v3 key belongs.
    AccessToken,
    /// Not 32 hex characters.
    BadFormat,
}

const TMDB_KEY_LEN: usize = 32;

#[must_use]
pub fn normalize(kind: InputKind, raw: &str) -> String {
    match kind {
        InputKind::Raw => raw.to_owned(),
        InputKind::Plain => raw.trim().to_owned(),
        InputKind::Url => normalize_url(raw),
        InputKind::Key => normalize_key(raw),
    }
}

/// The normalized committed draft, or `None` when nothing was committed or
/// normalization lands on the value already stored.
#[must_use]
pub fn changed_value(draft: Option<String>, kind: InputKind, current: &str) -> Option<String> {
    let next = normalize(kind, &draft?);
    if next == current {
        return None;
    }

    Some(next)
}

/// Trim, default to `http://` when no scheme is given, drop trailing slashes.
#[must_use]
pub fn normalize_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let bare = trimmed.trim_end_matches('/');
    if bare.contains("://") {
        return bare.to_owned();
    }

    format!("http://{bare}")
}

/// Drop every whitespace character; pasted keys often carry a newline or a space.
#[must_use]
pub fn normalize_key(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_whitespace()).collect()
}

#[must_use]
pub fn tmdb_key_hint(key: &str) -> Option<KeyHint> {
    if key.is_empty() {
        return None;
    }

    if key.starts_with("eyJ") {
        return Some(KeyHint::AccessToken);
    }

    let hex = key.len() == TMDB_KEY_LEN && key.chars().all(|c| c.is_ascii_hexdigit());
    if hex {
        return None;
    }

    Some(KeyHint::BadFormat)
}

/// DNS-over-HTTPS needs an `https://` endpoint; empty means "use the built-in resolvers".
#[must_use]
pub fn doh_url_ok(url: &str) -> bool {
    url.is_empty() || url.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_gets_scheme_and_loses_trailing_slash() {
        assert_eq!(normalize_url("  127.0.0.1:9117/ "), "http://127.0.0.1:9117");
        assert_eq!(normalize_url("https://jac.red//"), "https://jac.red");
        assert_eq!(normalize_url("http://host/path/"), "http://host/path");
    }

    #[test]
    fn empty_url_stays_empty() {
        assert_eq!(normalize_url("   "), "");
    }

    #[test]
    fn key_drops_all_whitespace() {
        assert_eq!(normalize_key(" abc\ndef \t"), "abcdef");
    }

    #[test]
    fn tmdb_key_hints() {
        let valid = "0123456789abcdef0123456789ABCDEF";
        let token = "eyJhbGciOiJIUzI1NiJ9";
        let not_hex = "0123456789abcdef0123456789abcdeg";

        assert_eq!(tmdb_key_hint(""), None);
        assert_eq!(tmdb_key_hint(valid), None);
        assert_eq!(tmdb_key_hint(token), Some(KeyHint::AccessToken));
        assert_eq!(tmdb_key_hint("0123"), Some(KeyHint::BadFormat));
        assert_eq!(tmdb_key_hint(not_hex), Some(KeyHint::BadFormat));
    }

    #[test]
    fn doh_requires_https() {
        assert!(doh_url_ok(""));
        assert!(doh_url_ok("https://dns.example/dns-query"));
        assert!(!doh_url_ok("http://dns.example/dns-query"));
    }
}
