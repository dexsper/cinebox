//! HTTP URL helpers shared by client crates.

use thiserror::Error;

/// Bad user-entered base URL.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum BaseUrlError {
    #[error("url is empty")]
    Empty,
}

/// Trim, add `http://` when the scheme is missing, strip a trailing slash.
///
/// # Errors
///
/// Returns [`BaseUrlError::Empty`] when `raw` is blank.
pub fn normalize_base_url(raw: &str) -> Result<String, BaseUrlError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BaseUrlError::Empty);
    }

    if trimmed.contains("://") {
        return Ok(trimmed.trim_end_matches('/').to_owned());
    }

    Ok(format!("http://{}", trimmed.trim_end_matches('/')))
}

/// Join `{base}/{path}` with a single slash.
#[must_use]
pub fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');

    format!("{base}/{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_error() {
        assert_eq!(normalize_base_url("  "), Err(BaseUrlError::Empty));
    }

    #[test]
    fn adds_http_and_strips_slash() -> Result<(), BaseUrlError> {
        let url = normalize_base_url("127.0.0.1:8090/")?;
        assert_eq!(url, "http://127.0.0.1:8090");

        Ok(())
    }

    #[test]
    fn keeps_https() -> Result<(), BaseUrlError> {
        let url = normalize_base_url("https://ts.example.com/")?;
        assert_eq!(url, "https://ts.example.com");

        Ok(())
    }

    #[test]
    fn join_avoids_double_slash() {
        assert_eq!(
            join_url("http://127.0.0.1:8090/", "/echo"),
            "http://127.0.0.1:8090/echo"
        );
    }
}
