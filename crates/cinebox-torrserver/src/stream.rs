//! `GET /stream/…` and `GET /play/{hash}/{id}` URL builders, plus preload wait.

use std::fmt::Write;
use std::time::Duration;

use cinebox_core::{join_url, normalize_base_url};

use super::cache::{cache_state, resume_window_progress};
use super::client::{apply_basic_auth, check_status, http_client, send_json};
use super::error::Error;
use super::status::TorrentStatus;

const PRELOAD_GET_SECS: u64 = 60;
const STAT_POLL_SECS: u64 = 1;
const STAT_POLL_MAX: u32 = 90;

/// How much to buffer ahead of a mid-file resume position, regardless of the
/// TorrServer cache size (whose readahead window can be much larger).
const RESUME_PRELOAD_BYTES: u64 = 64 << 20;

/// Pieces are coarse, so "ready" mirrors the stock preload gate.
const RESUME_READY_PERCENT: f64 = 95.0;

/// Query flag on `/stream/{fname}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamFlag {
    Play,
    Preload,
    Stat,
}

impl StreamFlag {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Preload => "preload",
            Self::Stat => "stat",
        }
    }
}

/// Percent-encode a path segment like JS `encodeURIComponent`.
fn encode_component(raw: &str) -> String {
    let mut out = String::new();
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

/// `GET /stream/{fname}?link={hash}&index={id}&{flag}`.
///
/// # Errors
///
/// Empty base URL.
pub fn stream_url(
    base_url: &str,
    file_path: &str,
    hash: &str,
    index: i32,
    flag: StreamFlag,
) -> Result<String, Error> {
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }
    let name = encode_component(file_name(file_path));
    Ok(format!(
        "{}/stream/{name}?link={hash}&index={index}&{}",
        base,
        flag.as_str()
    ))
}

/// `GET /play/{hash}/{id}`.
///
/// # Errors
///
/// Empty base URL or hash.
pub fn play_url(base_url: &str, hash: &str, index: i32) -> Result<String, Error> {
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }
    Ok(join_url(&base, &format!("play/{hash}/{index}")))
}

/// Progress while a preload wait is running.
#[derive(Clone, Copy, Debug)]
pub enum PreloadEvent {
    Progress {
        preloaded_bytes: i64,
        preload_size: i64,
        percent: f64,
    },
}

/// File a preload wait targets: `/stream/{file_path}?link={hash}&index={index}`.
#[derive(Clone, Copy, Debug)]
pub struct PreloadTarget<'a> {
    pub file_path: &'a str,
    pub hash: &'a str,
    pub index: i32,
}

impl PreloadTarget<'_> {
    fn url(&self, base_url: &str, flag: StreamFlag) -> Result<String, Error> {
        stream_url(base_url, self.file_path, self.hash, self.index, flag)
    }
}

/// `GET` the preload URL (starts buffer), then poll `?stat` until ~95%.
///
/// `on_event` receives a [`PreloadEvent::Progress`] for every poll iteration.
///
/// # Errors
///
/// Empty URL, HTTP failures, or timeout.
pub async fn wait_preload(
    base_url: &str,
    username: &str,
    password: &str,
    target: PreloadTarget<'_>,
    on_event: impl FnMut(PreloadEvent) + Send,
) -> Result<(), Error> {
    let preload = target.url(base_url, StreamFlag::Preload)?;
    let stat = target.url(base_url, StreamFlag::Stat)?;
    start_preload(&preload, username, password).await?;
    poll_stat_until_ready(&stat, username, password, on_event).await
}

/// Buffer around a mid-file resume position (TorrServer's `?preload` always
/// starts at byte 0, useless before a seek — YouROK/TorrServer#851).
///
/// Opens `?play` with `Range: bytes={offset}-`, which makes TorrServer create
/// a reader there and start filling pieces ahead of it. The connection is held
/// open (body unread) while `POST /cache` is polled; progress covers a fixed
/// [`RESUME_PRELOAD_BYTES`] window ahead of the reader.
///
/// A poll-budget timeout is soft: playback starts anyway with whatever has
/// buffered, so `Ok(())` is returned.
///
/// # Errors
///
/// Empty URL, HTTP failures, or no response headers within the GET budget.
pub async fn wait_preload_at_bytes(
    base_url: &str,
    username: &str,
    password: &str,
    target: PreloadTarget<'_>,
    offset_bytes: u64,
    mut on_event: impl FnMut(PreloadEvent) + Send,
) -> Result<(), Error> {
    let play = target.url(base_url, StreamFlag::Play)?;
    let client = http_client()?;

    // No request timeout: the body is never read and the connection must
    // stay open for the whole wait. Only the header wait is bounded.
    let range = format!("bytes={offset_bytes}-");
    let get = client.get(&play).header(reqwest::header::RANGE, range);
    let send = apply_basic_auth(get, username, password).send();
    let headers_budget = Duration::from_secs(PRELOAD_GET_SECS);
    let response = tokio::time::timeout(headers_budget, send)
        .await
        .map_err(|_| Error::PreloadTimeout)?
        .map_err(Error::Request)?;

    check_status(response.status())?;

    for _attempt in 0..STAT_POLL_MAX {
        let state = cache_state(base_url, username, password, target.hash).await?;
        let progress = resume_window_progress(&state, RESUME_PRELOAD_BYTES);

        on_event(PreloadEvent::Progress {
            preloaded_bytes: progress.completed_bytes,
            preload_size: progress.window_bytes,
            percent: progress.percent,
        });

        let window_known = progress.window_bytes > 0;
        if window_known && progress.percent >= RESUME_READY_PERCENT {
            break;
        }

        tokio::time::sleep(Duration::from_secs(STAT_POLL_SECS)).await;
    }

    // Dropping the response closes the connection and the TorrServer reader.
    drop(response);

    Ok(())
}

async fn start_preload(url: &str, username: &str, password: &str) -> Result<(), Error> {
    let client = http_client()?;
    let get = client.get(url).timeout(Duration::from_secs(PRELOAD_GET_SECS));
    let response = apply_basic_auth(get, username, password)
        .send()
        .await
        .map_err(Error::Request)?;
    
    check_status(response.status())?;
    Ok(())
}

async fn poll_stat_until_ready(
    url: &str,
    username: &str,
    password: &str,
    mut on_event: impl FnMut(PreloadEvent) + Send,
) -> Result<(), Error> {
    let client = http_client()?;

    for attempt in 0..STAT_POLL_MAX {
        let get = client.get(url).timeout(Duration::from_secs(10));
        let request = apply_basic_auth(get, username, password);
        let status: TorrentStatus = send_json(request).await?;

        on_event(PreloadEvent::Progress {
            preloaded_bytes: status.preloaded_bytes,
            preload_size: status.preload_size,
            percent: status.preload_percent(),
        });

        if status.preload_ready() {
            return Ok(());
        }

        let last = attempt + 1 == STAT_POLL_MAX;
        if last {
            return Err(Error::PreloadTimeout);
        }

        tokio::time::sleep(Duration::from_secs(STAT_POLL_SECS)).await;
    }
    Err(Error::PreloadTimeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_encodes_spaces_and_basename() {
        let url = stream_url(
            "http://127.0.0.1:8090/",
            r"Season 1\S01E01 Title.mkv",
            "abc",
            2,
            StreamFlag::Play,
        );
        let url = match url {
            Ok(url) => url,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(
            url,
            "http://127.0.0.1:8090/stream/S01E01%20Title.mkv?link=abc&index=2&play"
        );
    }

    #[test]
    fn play_joins_hash_and_id() {
        let url = play_url("http://127.0.0.1:8090", "deadbeef", 3);
        let url = match url {
            Ok(url) => url,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(url, "http://127.0.0.1:8090/play/deadbeef/3");
    }
}
