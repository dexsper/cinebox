//! `POST /cache` (action `get`): live cache window around active readers.
//!
//! TorrServer serializes `CacheState` with Go's default PascalCase field
//! names; map keys arrive as stringified piece ids.

use std::collections::HashMap;
use std::time::Duration;

use cinebox_core::{join_url, normalize_base_url};

use super::client::{apply_basic_auth, http_client, send_json};
use super::error::Error;

/// One reader window; every value is a piece index. `end` is exclusive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct ReaderState {
    #[serde(rename = "Start")]
    pub start: i64,
    #[serde(rename = "End")]
    pub end: i64,
    #[serde(rename = "Reader")]
    pub reader: i64,
}

/// One cached piece. Pieces with no downloaded bytes are absent from the map.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct PieceState {
    #[serde(rename = "Id")]
    pub id: i64,
    #[serde(rename = "Completed")]
    pub completed: bool,
}

/// Response of `POST /cache` (action `get`).
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct CacheState {
    #[serde(rename = "PiecesLength")]
    pub pieces_length: i64,
    #[serde(rename = "PiecesCount")]
    pub pieces_count: i64,
    #[serde(rename = "Pieces")]
    pub pieces: HashMap<i64, PieceState>,
    #[serde(rename = "Readers")]
    pub readers: Vec<ReaderState>,
}

/// How much of a fixed-size window ahead of the first reader is complete.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResumeProgress {
    pub completed_bytes: i64,
    pub window_bytes: i64,
    pub percent: f64,
}

const EMPTY_PROGRESS: ResumeProgress = ResumeProgress {
    completed_bytes: 0,
    window_bytes: 0,
    percent: 0.0,
};

/// `POST /cache` with `action: get`.
///
/// # Errors
///
/// Empty URL/hash, 404, or HTTP/JSON failures.
pub async fn cache_state(
    base_url: &str,
    username: &str,
    password: &str,
    hash: &str,
) -> Result<CacheState, Error> {
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    let url = join_url(&base, "cache");
    let client = http_client()?;
    let post = client.post(&url).timeout(Duration::from_secs(10));
    let request = apply_basic_auth(post, username, password).json(&serde_json::json!({
        "action": "get",
        "hash": hash,
    }));

    send_json(request).await
}

/// Completion of `needed_bytes` worth of pieces ahead of the first reader.
///
/// The window is `[reader, min(reader + needed, end))`, capped by the
/// reader's own readahead window so pieces TorrServer will never prioritize
/// are not waited on. Zero progress until a reader appears.
#[must_use]
pub fn resume_window_progress(state: &CacheState, needed_bytes: u64) -> ResumeProgress {
    if state.pieces_length <= 0 {
        return EMPTY_PROGRESS;
    }

    let Some(window) = state.readers.first() else {
        return EMPTY_PROGRESS;
    };

    let needed = ((needed_bytes as i64) / state.pieces_length).max(4);
    let window_end = (window.reader + needed).min(window.end);
    if window_end <= window.reader {
        return EMPTY_PROGRESS;
    }

    let total = window_end - window.reader;
    let ids = window.reader..window_end;
    let is_done = |id: &i64| state.pieces.get(id).is_some_and(|piece| piece.completed);
    let completed = ids.filter(|id| is_done(id)).count() as i64;

    let percent = (completed as f64) * 100.0 / (total as f64);

    ResumeProgress {
        completed_bytes: completed * state.pieces_length,
        window_bytes: total * state.pieces_length,
        percent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(id: i64, completed: bool) -> (i64, PieceState) {
        (id, PieceState { id, completed })
    }

    fn state_with(reader: ReaderState, pieces: Vec<(i64, PieceState)>) -> CacheState {
        CacheState {
            pieces_length: 8 << 20,
            pieces_count: 100,
            pieces: pieces.into_iter().collect(),
            readers: vec![reader],
        }
    }

    #[test]
    fn no_reader_means_zero_progress() {
        let state = CacheState::default();
        let progress = resume_window_progress(&state, 64 << 20);
        assert!((progress.percent - 0.0).abs() < f64::EPSILON);
        assert_eq!(progress.window_bytes, 0);
    }

    #[test]
    fn counts_completed_pieces_ahead_of_reader() {
        // 64 MiB / 8 MiB pieces = 8 needed; window [10, 18).
        let reader = ReaderState {
            start: 8,
            end: 40,
            reader: 10,
        };
        let pieces = vec![piece(10, true), piece(11, true), piece(12, false)];
        let state = state_with(reader, pieces);

        let progress = resume_window_progress(&state, 64 << 20);
        assert_eq!(progress.window_bytes, 8 * (8 << 20));
        assert_eq!(progress.completed_bytes, 2 * (8 << 20));
        assert!((progress.percent - 25.0).abs() < 0.01, "{}", progress.percent);
    }

    #[test]
    fn window_is_capped_by_reader_end() {
        // Reader window ends 2 pieces ahead: wait only for those two.
        let reader = ReaderState {
            start: 0,
            end: 12,
            reader: 10,
        };
        let pieces = vec![piece(10, true), piece(11, true), piece(30, true)];
        let state = state_with(reader, pieces);

        let progress = resume_window_progress(&state, 64 << 20);
        assert_eq!(progress.window_bytes, 2 * (8 << 20));
        assert!((progress.percent - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_torrserver_pascal_case_json() {
        let fixture = r#"{
            "Hash": "abc",
            "Capacity": 268435456,
            "Filled": 1000,
            "PiecesLength": 4194304,
            "PiecesCount": 1500,
            "Pieces": {
                "42": { "Id": 42, "Length": 4194304, "Size": 4194304, "Completed": true, "Priority": 5 }
            },
            "Readers": [ { "Start": 40, "End": 55, "Reader": 42 } ]
        }"#;

        let state: CacheState = serde_json::from_str(fixture)
            .unwrap_or_else(|error| panic!("fixture: {error}"));

        assert_eq!(state.pieces_length, 4_194_304);
        assert_eq!(state.readers.len(), 1);
        assert_eq!(state.readers[0].reader, 42);
        assert!(state.pieces[&42].completed);
    }

    #[test]
    fn empty_object_from_nil_cache_state_parses() {
        // TorrServer replies `{}` when the torrent has no cache yet.
        let state: CacheState = serde_json::from_str("{}")
            .unwrap_or_else(|error| panic!("fixture: {error}"));

        assert_eq!(state, CacheState::default());
    }
}
