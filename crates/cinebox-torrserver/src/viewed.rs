//! `POST /viewed` list / set.

use std::time::Duration;

use cinebox_core::{join_url, normalize_base_url};

use super::client::{apply_basic_auth, http_client, send_json};
use super::error::Error;

/// One row from `POST /viewed` action `list`. File index is 1-based.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Viewed {
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub file_index: i32,
    #[serde(default)]
    pub timecode: f64,
}

/// `POST /viewed` with `action: list`. Missing history is an empty vec, not an error.
///
/// # Errors
///
/// Empty URL or HTTP/JSON failures.
pub async fn viewed_list(
    base_url: &str,
    username: &str,
    password: &str,
    hash: &str,
) -> Result<Vec<Viewed>, Error> {
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    let url = join_url(&base, "viewed");
    let client = http_client()?;

    let post = client.post(&url).timeout(Duration::from_secs(10));
    let request = apply_basic_auth(post, username, password).json(&serde_json::json!({
        "action": "list",
        "hash": hash,
    }));

    send_json(request).await
}

/// `POST /viewed` with `action: set`. Server stores `timecode` only when TrackTimecode is on.
///
/// # Errors
///
/// Empty URL/hash or HTTP failures.
pub async fn viewed_set(
    base_url: &str,
    username: &str,
    password: &str,
    hash: &str,
    file_index: i32,
    timecode: f64,
) -> Result<(), Error> {
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    let url = join_url(&base, "viewed");
    let client = http_client()?;

    let post = client.post(&url).timeout(Duration::from_secs(10));
    let request = apply_basic_auth(post, username, password).json(&serde_json::json!({
        "action": "set",
        "hash": hash,
        "file_index": file_index,
        "timecode": timecode,
    }));

    let response = request.send().await.map_err(Error::Request)?;
    super::client::check_status(response.status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_viewed_list_fixture() {
        // Trimmed real `POST /viewed action=list` body.
        let fixture = r#"[
            { "hash": "abc", "file_index": 1, "timecode": 512.25 },
            { "hash": "abc", "file_index": 3 }
        ]"#;

        let rows: Vec<Viewed> = serde_json::from_str(fixture)
            .unwrap_or_else(|error| panic!("fixture: {error}"));

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].file_index, 1);
        assert!((rows[0].timecode - 512.25).abs() < f64::EPSILON);
        assert!((rows[1].timecode - 0.0).abs() < f64::EPSILON);
    }
}
