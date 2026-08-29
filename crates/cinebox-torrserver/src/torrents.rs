//! `POST /torrents`: add, get, list, drop, and open (add + wait files + viewed).

use std::time::Duration;

use cinebox_core::{join_url, normalize_base_url};
use serde::Serialize;

use super::client::{apply_basic_auth, http_client, send_json};
use super::error::Error;
use super::status::{TorrentStatus, files_for_list};
use super::viewed::{Viewed, viewed_list};

const JSON_TIMEOUT: Duration = Duration::from_secs(20);
const FILE_POLL_INTERVAL: Duration = Duration::from_secs(2);
const FILE_POLL_MAX: u32 = 45;

#[derive(Debug, Clone, serde::Deserialize)]
struct ListedRaw {
    hash: Option<String>,
    title: Option<String>,
}

/// One torrent already on TorrServer (`POST /torrents` action `list`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedTorrent {
    pub hash: String,
    pub title: String,
}

/// Fields for `POST /torrents` action `add`.
#[derive(Debug, Clone)]
pub struct AddSpec {
    pub link: String,
    pub title: String,
    pub poster: String,
    pub category: String,
    pub save_to_db: bool,
}

#[derive(Serialize)]
struct AddBody<'a> {
    action: &'static str,
    link: &'a str,
    title: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    poster: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    category: &'a str,
    save_to_db: bool,
}

/// One playable file after add, with optional resume timecode.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenedFile {
    pub id: i32,
    pub path: String,
    pub length: u64,
    pub timecode: f64,
}

/// Torrent ready to show as a file/episode list.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenedTorrent {
    pub hash: String,
    pub files: Vec<OpenedFile>,
    pub resume_id: Option<i32>,
}

fn torrents_url(base_url: &str) -> Result<String, Error> {
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    Ok(join_url(&base, "torrents"))
}

/// `POST /torrents` with `action: list`. Failures here must not block indexer search.
///
/// # Errors
///
/// Empty URL, HTTP failures, or JSON that is not an array of torrent objects.
pub async fn list(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<Vec<ListedTorrent>, Error> {
    let url = torrents_url(base_url)?;
    let client = http_client(Duration::from_secs(15))?;
    let request = apply_basic_auth(client.post(&url), username, password)
        .json(&serde_json::json!({ "action": "list" }));

    let parsed: Vec<ListedRaw> = send_json(request).await?;
    Ok(parsed
        .into_iter()
        .filter_map(|row| {
            let hash = row.hash.filter(|h| !h.is_empty())?;
            Some(ListedTorrent {
                title: row.title.unwrap_or_default(),
                hash,
            })
        })
        .collect())
}

/// `POST /torrents` with `action: add`. `file_stats` is often empty until `get` is polled.
///
/// # Errors
///
/// Empty URL/link or HTTP/JSON failures.
pub async fn add(
    base_url: &str,
    username: &str,
    password: &str,
    spec: &AddSpec,
) -> Result<TorrentStatus, Error> {
    if spec.link.trim().is_empty() {
        return Err(Error::EmptyLink);
    }

    let url = torrents_url(base_url)?;
    let client = http_client(JSON_TIMEOUT)?;
    let body = AddBody {
        action: "add",
        link: spec.link.trim(),
        title: spec.title.as_str(),
        poster: spec.poster.as_str(),
        category: spec.category.as_str(),
        save_to_db: spec.save_to_db,
    };

    let request = apply_basic_auth(client.post(&url), username, password).json(&body);
    send_json(request).await
}

/// `POST /torrents` with `action: get`.
///
/// # Errors
///
/// Empty URL/hash, 404, or HTTP/JSON failures.
pub async fn get(
    base_url: &str,
    username: &str,
    password: &str,
    hash: &str,
) -> Result<TorrentStatus, Error> {
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let url = torrents_url(base_url)?;
    let client = http_client(JSON_TIMEOUT)?;
    let request =
        apply_basic_auth(client.post(&url), username, password).json(&serde_json::json!({
            "action": "get",
            "hash": hash,
        }));

    send_json(request).await
}

/// Poll `get` until `file_stats` is non-empty (Lampa: 45 × 2s).
///
/// # Errors
///
/// Empty URL/hash, HTTP failures, or timeout.
pub async fn wait_files(
    base_url: &str,
    username: &str,
    password: &str,
    hash: &str,
) -> Result<TorrentStatus, Error> {
    for attempt in 0..FILE_POLL_MAX {
        match get(base_url, username, password, hash).await {
            Ok(status) if !status.file_stats.is_empty() => return Ok(status),
            Ok(_) => {}
            Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }

        let last = attempt + 1 == FILE_POLL_MAX;
        if last {
            return Err(Error::FilesTimeout);
        }

        tokio::time::sleep(FILE_POLL_INTERVAL).await;
    }
    Err(Error::FilesTimeout)
}

/// `POST /torrents` with `action: drop` (unload, keep in DB if saved).
///
/// # Errors
///
/// Empty URL/hash or HTTP failures.
pub async fn drop_torrent(
    base_url: &str,
    username: &str,
    password: &str,
    hash: &str,
) -> Result<(), Error> {
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let url = torrents_url(base_url)?;
    let client = http_client(JSON_TIMEOUT)?;
    let request =
        apply_basic_auth(client.post(&url), username, password).json(&serde_json::json!({
            "action": "drop",
            "hash": hash,
        }));

    let response = request.send().await.map_err(Error::Request)?;
    super::client::check_status(response.status())
}

/// Add a magnet, wait for files, optionally attach `/viewed` timecodes.
///
/// # Errors
///
/// Empty URL/link, HTTP failures, missing hash, or file-list timeout.
pub async fn open_magnet(
    base_url: &str,
    username: &str,
    password: &str,
    spec: &AddSpec,
    track_timecode: bool,
) -> Result<OpenedTorrent, Error> {
    let added = add(base_url, username, password, spec).await?;
    if added.hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let hash = added.hash.clone();
    let status = match wait_or_use(base_url, username, password, added).await {
        Ok(status) => status,
        Err(error) => {
            let _ = drop_torrent(base_url, username, password, &hash).await;
            return Err(error);
        }
    };

    let viewed = if track_timecode {
        viewed_list(base_url, username, password, &hash)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(opened_from(status, &viewed))
}

async fn wait_or_use(
    base_url: &str,
    username: &str,
    password: &str,
    added: TorrentStatus,
) -> Result<TorrentStatus, Error> {
    if !added.file_stats.is_empty() {
        return Ok(added);
    }

    wait_files(base_url, username, password, &added.hash).await
}

fn opened_from(status: TorrentStatus, viewed: &[Viewed]) -> OpenedTorrent {
    let files: Vec<OpenedFile> = files_for_list(&status.file_stats)
        .into_iter()
        .map(|file| OpenedFile {
            id: file.id,
            path: file.path,
            length: file.length.max(0) as u64,
            timecode: timecode_for(viewed, file.id),
        })
        .collect();

    let resume_id = files
        .iter()
        .find(|file| file.timecode > 0.0)
        .map(|file| file.id);

    OpenedTorrent {
        hash: status.hash,
        files,
        resume_id,
    }
}

fn timecode_for(viewed: &[Viewed], file_id: i32) -> f64 {
    viewed
        .iter()
        .find(|row| row.file_index == file_id)
        .map(|row| row.timecode)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::FileStat;

    #[test]
    fn resume_is_first_started_file() {
        let status = TorrentStatus {
            hash: String::from("ab"),
            title: String::new(),
            stat: 3,
            preloaded_bytes: 0,
            preload_size: 0,
            download_speed: 0.0,
            active_peers: 0,
            total_peers: 0,
            file_stats: vec![
                FileStat {
                    id: 1,
                    path: String::from("a.mkv"),
                    length: 10,
                },
                FileStat {
                    id: 2,
                    path: String::from("b.mkv"),
                    length: 10,
                },
            ],
        };
        
        let viewed = vec![Viewed {
            hash: String::from("ab"),
            file_index: 2,
            timecode: 12.5,
        }];

        let opened = opened_from(status, &viewed);
        assert_eq!(opened.resume_id, Some(2));
        assert_eq!(opened.files[1].timecode, 12.5);
    }
}
