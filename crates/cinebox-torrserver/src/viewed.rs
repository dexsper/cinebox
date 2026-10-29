//! `POST /viewed` list / set.

use std::time::Duration;

use cinebox_core::join_url;

use super::client::{http_client, send_json};
use super::error::Error;
use super::server::Server;

/// How far a server position may drift from the one this device saw and still count as its own.
const SEEN_TOLERANCE_SECS: f64 = 1.0;

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
pub async fn viewed_list(server: &Server, hash: &str) -> Result<Vec<Viewed>, Error> {
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    list_request(server, hash).await
}

/// `POST /viewed` with `action: list` and no hash: every row of every torrent in one request.
///
/// # Errors
///
/// Empty URL or HTTP/JSON failures.
pub async fn viewed_all(server: &Server) -> Result<Vec<Viewed>, Error> {
    list_request(server, "").await
}

async fn list_request(server: &Server, hash: &str) -> Result<Vec<Viewed>, Error> {
    let base = server.base()?;
    let url = join_url(&base, "viewed");
    let client = http_client()?;

    let post = client.post(&url).timeout(Duration::from_secs(10));
    let request = server.authorize(post).json(&serde_json::json!({
        "action": "list",
        "hash": hash,
    }));

    // The server answers `null` instead of `[]` when it has no rows at all.
    let rows: Option<Vec<Viewed>> = send_json(request).await?;

    Ok(rows.unwrap_or_default())
}

/// `true` when another client wrote `server` after this device last saw or sent `seen` for the file.
///
/// A zero is never foreign: a server with `TrackTimecode` off stores zero for every `set`, our own included.
#[must_use]
pub fn is_foreign_timecode(server: f64, seen: Option<f64>) -> bool {
    if server <= 0.0 {
        return false;
    }

    seen.is_none_or(|seen| (server - seen).abs() > SEEN_TOLERANCE_SECS)
}

/// The position to resume a file at, seconds: the server's when another client moved it since this device
/// last saw it, else the local one, else the server's.
#[must_use]
pub fn merge_timecode(local: Option<f64>, server: f64, seen: Option<f64>) -> f64 {
    if is_foreign_timecode(server, seen) {
        return server;
    }

    local.unwrap_or(server)
}

/// `POST /viewed` with `action: set`. Server stores `timecode` only when `TrackTimecode` is on.
///
/// # Errors
///
/// Empty URL/hash or HTTP failures.
pub async fn viewed_set(server: &Server, hash: &str, file_index: i32, timecode: f64) -> Result<(), Error> {
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let base = server.base()?;
    let url = join_url(&base, "viewed");
    let client = http_client()?;

    let post = client.post(&url).timeout(Duration::from_secs(10));
    let request = server.authorize(post).json(&serde_json::json!({
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

        let rows: Vec<Viewed> = serde_json::from_str(fixture).unwrap_or_else(|error| panic!("fixture: {error}"));

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].file_index, 1);
        assert!((rows[0].timecode - 512.25).abs() < f64::EPSILON);
        assert!((rows[1].timecode - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_position_another_client_moved_wins_over_the_local_one() {
        let merged = merge_timecode(Some(600.0), 1200.0, Some(300.0));

        assert!((merged - 1200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_local_position_wins_while_the_server_holds_what_this_device_sent() {
        let merged = merge_timecode(Some(900.0), 300.4, Some(300.0));

        assert!((merged - 900.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_position_never_seen_here_is_foreign() {
        assert!(is_foreign_timecode(42.0, None));
        assert!((merge_timecode(Some(10.0), 42.0, None) - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_zero_from_the_server_is_never_foreign() {
        assert!(!is_foreign_timecode(0.0, Some(300.0)));
        assert!((merge_timecode(Some(300.0), 0.0, Some(300.0)) - 300.0).abs() < f64::EPSILON);
    }
}
