//! `POST /torrents`: add, get, list, drop, and open (add + wait files + viewed).

use std::time::Duration;

use cinebox_core::join_url;
use serde::Serialize;

use super::client::{http_client, send_json};
use super::error::Error;
use super::server::Server;
use super::status::{FileStat, TorrentStatus, files_for_list};
use super::viewed::{Viewed, viewed_list};

const JSON_TIMEOUT: Duration = Duration::from_secs(20);
const FILE_POLL_INTERVAL: Duration = Duration::from_secs(2);
const FILE_POLL_MAX: u32 = 45;

/// The longest `[TAG] ` prefix a client may put in front of a title.
const TITLE_TAG_MAX: usize = 16;

#[derive(Debug, Clone, serde::Deserialize)]
struct ListedRaw {
    hash: Option<String>,
    title: Option<String>,
    #[serde(default)]
    poster: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    data: String,
    #[serde(default)]
    timestamp: i64,
    #[serde(default)]
    torrent_size: i64,
    #[serde(default)]
    file_stats: Vec<FileStat>,
}

/// One torrent already on TorrServer (`POST /torrents` action `list`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedTorrent {
    pub hash: String,

    /// Without a client's `[TAG] ` prefix.
    pub title: String,

    pub poster: String,

    pub category: String,

    /// Free-form client data, see [`crate::decode_data`].
    pub data: String,

    /// When the torrent was added or last loaded, unix seconds; `0` when unknown.
    pub timestamp: i64,

    pub size_bytes: u64,

    /// Empty for a torrent that sits only in the server's database.
    pub file_stats: Vec<FileStat>,
}

/// Fields for `POST /torrents` action `add`.
#[derive(Debug, Clone)]
pub struct AddSpec {
    pub link: String,
    pub title: String,
    pub poster: String,
    pub category: String,
    /// Client data stored with the torrent, see [`crate::encode_data`]; empty sends none.
    pub data: String,
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
    #[serde(skip_serializing_if = "str::is_empty")]
    data: &'a str,
    save_to_db: bool,
}

/// Fields for `POST /torrents` action `set`.
///
/// The server replaces title, poster and category with what is sent, even when empty, and keeps the old
/// data when `data` is empty. An empty title makes it load the torrent to read its name, so send one.
#[derive(Debug, Clone, Copy)]
pub struct MetaSpec<'a> {
    pub hash: &'a str,
    pub title: &'a str,
    pub poster: &'a str,
    pub category: &'a str,
    pub data: &'a str,
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

fn torrents_url(server: &Server) -> Result<String, Error> {
    Ok(join_url(&server.base()?, "torrents"))
}

/// `POST /torrents` with `action: list`. Failures here must not block indexer search.
///
/// # Errors
///
/// Empty URL, HTTP failures, or JSON that is not an array of torrent objects.
pub async fn list(server: &Server) -> Result<Vec<ListedTorrent>, Error> {
    let url = torrents_url(server)?;
    let client = http_client()?;
    let post = client.post(&url).timeout(Duration::from_secs(15));
    let request = server.authorize(post).json(&serde_json::json!({ "action": "list" }));

    let parsed: Vec<ListedRaw> = send_json(request).await?;
    Ok(parsed.into_iter().filter_map(listed_from).collect())
}

fn listed_from(row: ListedRaw) -> Option<ListedTorrent> {
    let hash = row.hash.filter(|hash| !hash.is_empty())?;
    let title = row.title.unwrap_or_default();

    Some(ListedTorrent {
        hash,
        title: without_title_tag(&title).to_owned(),
        poster: row.poster,
        category: row.category,
        data: row.data,
        timestamp: row.timestamp,
        size_bytes: u64::try_from(row.torrent_size).unwrap_or(0),
        file_stats: row.file_stats,
    })
}

/// `title` without a leading `[TAG] ` some clients mark their torrents with.
fn without_title_tag(title: &str) -> &str {
    let Some(rest) = title.strip_prefix('[') else { return title };
    let Some((tag, name)) = rest.split_once("] ") else { return title };

    let is_tag = (1..=TITLE_TAG_MAX).contains(&tag.len());
    let is_marker = tag.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit());
    if !is_tag || !is_marker || name.trim().is_empty() {
        return title;
    }

    name.trim_start()
}

/// `POST /torrents` with `action: set`: replaces a torrent's title, poster and category, and its data
/// when `data` is not empty.
///
/// # Errors
///
/// Empty URL/hash or HTTP failures.
pub async fn set_meta(server: &Server, meta: MetaSpec<'_>) -> Result<(), Error> {
    if meta.hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let url = torrents_url(server)?;
    let client = http_client()?;
    let post = client.post(&url).timeout(JSON_TIMEOUT);
    let request = server.authorize(post).json(&serde_json::json!({
        "action": "set",
        "hash": meta.hash,
        "title": meta.title,
        "poster": meta.poster,
        "category": meta.category,
        "data": meta.data,
    }));

    let response = request.send().await.map_err(Error::Request)?;
    super::client::check_status(response.status())
}

/// `POST /torrents` with `action: add`. `file_stats` is often empty until `get` is polled.
///
/// # Errors
///
/// Empty URL/link or HTTP/JSON failures.
pub async fn add(server: &Server, spec: &AddSpec) -> Result<TorrentStatus, Error> {
    if spec.link.trim().is_empty() {
        return Err(Error::EmptyLink);
    }

    let url = torrents_url(server)?;
    let client = http_client()?;
    let body = AddBody {
        action: "add",
        link: spec.link.trim(),
        title: spec.title.as_str(),
        poster: spec.poster.as_str(),
        category: spec.category.as_str(),
        data: spec.data.as_str(),
        save_to_db: spec.save_to_db,
    };

    let post = client.post(&url).timeout(JSON_TIMEOUT);
    let request = server.authorize(post).json(&body);
    send_json(request).await
}

/// `POST /torrents` with `action: get`. For a torrent kept only in the server database this starts
/// loading it, so it is not a way to read files of a torrent nobody plays.
///
/// # Errors
///
/// Empty URL/hash, 404, or HTTP/JSON failures.
pub async fn get(server: &Server, hash: &str) -> Result<TorrentStatus, Error> {
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let url = torrents_url(server)?;
    let client = http_client()?;
    let post = client.post(&url).timeout(JSON_TIMEOUT);
    let request = server.authorize(post).json(&serde_json::json!({
        "action": "get",
        "hash": hash,
    }));

    send_json(request).await
}

/// Poll `get` until `file_stats` is non-empty (45 × 2s).
///
/// # Errors
///
/// Empty URL/hash, HTTP failures, or timeout.
pub async fn wait_files(server: &Server, hash: &str) -> Result<TorrentStatus, Error> {
    for attempt in 0..FILE_POLL_MAX {
        match get(server, hash).await {
            Ok(status) if !status.file_stats.is_empty() => return Ok(status),
            Ok(_) | Err(Error::NotFound) => {}
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
pub async fn drop_torrent(server: &Server, hash: &str) -> Result<(), Error> {
    if hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let url = torrents_url(server)?;
    let client = http_client()?;
    let post = client.post(&url).timeout(JSON_TIMEOUT);
    let request = server.authorize(post).json(&serde_json::json!({
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
pub async fn open_magnet(server: &Server, spec: &AddSpec, track_timecode: bool) -> Result<OpenedTorrent, Error> {
    let added = add(server, spec).await?;
    if added.hash.is_empty() {
        return Err(Error::EmptyHash);
    }

    let hash = added.hash.clone();
    let status = match wait_or_use(server, added).await {
        Ok(status) => status,
        Err(error) => {
            // The wait's error is the one to report; a torrent left loaded only holds server memory.
            drop(drop_torrent(server, &hash).await);
            return Err(error);
        }
    };

    let viewed = if track_timecode { viewed_list(server, &hash).await.unwrap_or_default() } else { Vec::new() };

    Ok(opened_from(status, &viewed))
}

async fn wait_or_use(server: &Server, added: TorrentStatus) -> Result<TorrentStatus, Error> {
    if !added.file_stats.is_empty() {
        return Ok(added);
    }

    wait_files(server, &added.hash).await
}

fn opened_from(status: TorrentStatus, viewed: &[Viewed]) -> OpenedTorrent {
    let files: Vec<OpenedFile> = files_for_list(&status.file_stats)
        .into_iter()
        .map(|file| OpenedFile {
            id: file.id,
            path: file.path,
            length: u64::try_from(file.length).unwrap_or(0),
            timecode: timecode_for(viewed, file.id),
        })
        .collect();

    let resume_id = files.iter().find(|file| file.timecode > 0.0).map(|file| file.id);

    OpenedTorrent { hash: status.hash, files, resume_id }
}

fn timecode_for(viewed: &[Viewed], file_id: i32) -> f64 {
    viewed.iter().find(|row| row.file_index == file_id).map_or(0.0, |row| row.timecode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_tag_is_cut_from_the_title() {
        assert_eq!(without_title_tag("[CLIENT] Дюна"), "Дюна");
        assert_eq!(without_title_tag("[TS2] Show S01"), "Show S01");
    }

    #[test]
    fn a_bracketed_name_is_kept() {
        let cases = ["[Hi10] subs", "[Хорошее] кино", "[] empty", "[TAG]glued", "[TAG] ", "Plain title"];

        for title in cases {
            assert_eq!(without_title_tag(title), title, "{title}");
        }
    }

    #[test]
    fn a_saved_torrent_lists_with_its_card() {
        let json = r#"[{"title":"[CLIENT] Дюна","category":"movie","poster":"https://p/x.jpg",
            "data":"{\"movie\":{\"id\":438631}}","timestamp":1700000000,"hash":"ab",
            "stat":5,"stat_string":"Torrent in db","torrent_size":2048}]"#;

        let rows: Vec<ListedRaw> = serde_json::from_str(json).unwrap_or_else(|error| panic!("fixture: {error}"));
        let listed: Vec<ListedTorrent> = rows.into_iter().filter_map(listed_from).collect();

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Дюна");
        assert_eq!(listed[0].timestamp, 1_700_000_000);
        assert_eq!(listed[0].size_bytes, 2048);
        assert_eq!(listed[0].file_stats, Vec::new());
        assert_eq!(crate::decode_data(&listed[0].data).map(|card| card.id), Some(438_631));
    }

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
                FileStat { id: 1, path: String::from("a.mkv"), length: 10 },
                FileStat { id: 2, path: String::from("b.mkv"), length: 10 },
            ],
        };

        let viewed = vec![Viewed { hash: String::from("ab"), file_index: 2, timecode: 12.5 }];

        let opened = opened_from(status, &viewed);
        assert_eq!(opened.resume_id, Some(2));
        assert_eq!(opened.files[1].timecode, 12.5);
    }
}
