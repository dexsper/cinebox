//! Jackett JSON and Prowlarr REST search.

use std::time::Duration;

use cinebox_core::{MediaKind, ParserKind, join_url, normalize_base_url};
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::Error;

/// Card search sent to Jackett extras / Prowlarr query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub query: String,
    pub title: String,
    pub original_title: String,
    pub year: Option<u16>,
    pub kind: MediaKind,
    pub is_anime: bool,
    pub genres: Vec<String>,
}

/// Normalized indexer row (Jackett + Prowlarr).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub title: String,
    pub tracker: String,
    pub size_bytes: u64,
    pub seeders: u32,
    pub peers: u32,
    pub magnet: String,
    pub published: String,
}

#[derive(Debug, Deserialize)]
struct JackettResults {
    #[serde(default, rename = "Results", alias = "results")]
    results: Vec<Value>,
    #[serde(default, rename = "Indexers", alias = "indexers")]
    indexers: Vec<JackettIndexer>,
}

#[derive(Debug, Deserialize)]
struct JackettIndexer {
    #[serde(default, rename = "Name", alias = "name")]
    name: String,
    #[serde(default, rename = "Error", alias = "error")]
    error: Option<String>,
}

#[derive(Clone, Copy)]
enum JackettMode {
    /// Aggregator extras (`title`, `year`, `is_serial`) without `Category[]`.
    Full,
    /// Vanilla Jackett: `Query` only.
    QueryOnly,
}

fn json_u64(value: &Value) -> u64 {
    match value {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_i64().and_then(|v| u64::try_from(v).ok()))
            .or_else(|| n.as_f64().map(|v| v.max(0.0) as u64))
            .unwrap_or(0),
        Value::String(s) => parse_size_text(s),
        _ => 0,
    }
}

fn parse_size_text(raw: &str) -> u64 {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return 0;
    }
    if let Ok(n) = trimmed.parse::<u64>() {
        return n;
    }
    let compact: String = trimmed
        .chars()
        .filter(|ch| *ch != ',' && *ch != ' ' && *ch != '\u{00a0}')
        .collect();
    if let Ok(n) = compact.parse::<u64>() {
        return n;
    }
    if let Ok(n) = trimmed.replace(',', ".").parse::<f64>()
        && n.is_finite()
        && n >= 0.0
    {
        return n as u64;
    }
    parse_size_label(trimmed).unwrap_or(0)
}

fn parse_size_label(raw: &str) -> Option<u64> {
    let lower = raw.trim().to_lowercase().replace(',', ".");
    let split = lower
        .char_indices()
        .find(|(_, ch)| ch.is_alphabetic() || *ch == 'б' || *ch == 'Б');
    let (num_part, unit_part) = match split {
        Some((index, _)) => lower.split_at(index),
        None => return None,
    };
    let num: f64 = num_part.trim().parse().ok()?;
    if !num.is_finite() || num < 0.0 {
        return None;
    }
    let unit = unit_part.trim();
    let mul = if unit.starts_with("tib") || unit.starts_with("tb") || unit.starts_with('т') {
        1024.0 * 1024.0 * 1024.0 * 1024.0
    } else if unit.starts_with("gib") || unit.starts_with("gb") || unit.starts_with('г') {
        1024.0 * 1024.0 * 1024.0
    } else if unit.starts_with("mib") || unit.starts_with("mb") || unit.starts_with('м') {
        1024.0 * 1024.0
    } else if unit.starts_with("kib") || unit.starts_with("kb") || unit.starts_with('к') {
        1024.0
    } else {
        return None;
    };
    Some((num * mul).round() as u64)
}

fn row_size_bytes(raw: &Value) -> u64 {
    for key in ["Size", "size", "fileSize", "FileSize", "bytes"] {
        let n = json_u64(raw.get(key).unwrap_or(&Value::Null));
        if n > 0 {
            return n;
        }
    }
    0
}

fn json_u32(value: &Value) -> u32 {
    u32::try_from(json_u64(value)).unwrap_or(u32::MAX)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn format_publish_date(raw: &str) -> String {
    let y: u16 = match raw.get(0..4).and_then(|part| part.parse().ok()) {
        Some(year) => year,
        None => return String::new(),
    };
    let month: usize = match raw.get(5..7).and_then(|part| part.parse::<usize>().ok()) {
        Some(month) if (1..=12).contains(&month) => month,
        _ => return String::new(),
    };
    let day: u8 = match raw.get(8..10).and_then(|part| part.parse().ok()) {
        Some(day) if day > 0 => day,
        _ => return String::new(),
    };
    let Some(name) = MONTHS.get(month.saturating_sub(1)) else {
        return String::new();
    };
    format!("{day} {name} {y}")
}

/// Strip Lucene operators so titles like `Spider-Man: Brand New Day` stay a phrase.
#[must_use]
fn sanitize_query(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        let mapped = match ch {
            ':' | '/' | '\\' | '"' | '\'' | '[' | ']' | '{' | '}' | '!' | '(' | ')' | '&' | '|' => {
                ' '
            }
            other => other,
        };
        if mapped.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(mapped);
        }
    }
    out.trim().to_owned()
}

fn search_text(raw: &str) -> String {
    let cleaned = sanitize_query(raw);
    if cleaned.is_empty() {
        raw.trim().to_owned()
    } else {
        cleaned
    }
}

fn log_jackett_indexers(parsed: &JackettResults, query: &str) {
    for indexer in &parsed.indexers {
        let Some(error) = indexer.error.as_deref().filter(|text| !text.is_empty()) else {
            continue;
        };
        warn!(
            indexer = %indexer.name,
            error,
            query,
            "jackett indexer failed"
        );
    }
}

fn text_field(obj: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = obj.get(*key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.to_owned();
            }
        }
    }
    String::new()
}

fn http_client(use_system_proxy: bool) -> Result<reqwest::Client, Error> {
    crate::apply_system_proxy(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .connect_timeout(Duration::from_secs(8)),
        use_system_proxy,
    )
    .build()
    .map_err(Error::Client)
}

/// Search Jackett or Prowlarr. The API key is never included in error text.
///
/// # Errors
///
/// Empty URL, HTTP failures, or JSON that does not match the expected shape.
pub async fn search(
    kind: ParserKind,
    base_url: &str,
    api_key: &str,
    query: &SearchQuery,
    use_system_proxy: bool,
) -> Result<Vec<Hit>, Error> {
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    let q = search_text(&query.query);
    let hits = match kind {
        ParserKind::Jackett => search_jackett(&base, api_key, query, &q, use_system_proxy).await?,
        ParserKind::Prowlarr => {
            search_prowlarr(&base, api_key, query, &q, use_system_proxy).await?
        }
    };
    info!(
        parser = ?kind,
        query = %q,
        n = hits.len(),
        "indexer search"
    );
    Ok(hits)
}

async fn search_jackett(
    base: &str,
    api_key: &str,
    query: &SearchQuery,
    q: &str,
    use_system_proxy: bool,
) -> Result<Vec<Hit>, Error> {
    let url = join_url(base, "api/v2.0/indexers/all/results");
    let client = http_client(use_system_proxy)?;
    let parsed = jackett_get(&client, &url, api_key, query, q, JackettMode::Full).await?;
    log_jackett_indexers(&parsed, q);
    let mut hits: Vec<Hit> = parsed.results.iter().filter_map(hit_from_jackett).collect();
    if hits.is_empty() {
        warn!(query = %q, "jackett empty; retrying with Query only");
        let parsed = jackett_get(&client, &url, api_key, query, q, JackettMode::QueryOnly).await?;
        log_jackett_indexers(&parsed, q);
        hits = parsed.results.iter().filter_map(hit_from_jackett).collect();
        if hits.is_empty() {
            warn!(
                query = %q,
                indexers = parsed.indexers.len(),
                "jackett returned no results"
            );
        }
    }
    Ok(hits)
}

async fn jackett_get(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    query: &SearchQuery,
    q: &str,
    mode: JackettMode,
) -> Result<JackettResults, Error> {
    let mut request = client.get(url).query(&[("apikey", api_key), ("Query", q)]);
    let year = query.year.map(|year| year.to_string());
    let genres = if query.genres.is_empty() {
        None
    } else {
        Some(query.genres.join(","))
    };
    if matches!(mode, JackettMode::Full) {
        let is_serial = match query.kind {
            MediaKind::Tv => "2",
            _ => "1",
        };
        request = request.query(&[("is_serial", is_serial)]);
        if !query.title.is_empty() {
            request = request.query(&[("title", query.title.as_str())]);
        }
        if !query.original_title.is_empty() {
            request = request.query(&[("title_original", query.original_title.as_str())]);
        }
        if let Some(year) = year.as_deref() {
            request = request.query(&[("year", year)]);
        }
        if let Some(genres) = genres.as_deref() {
            request = request.query(&[("genres", genres)]);
        }
    }
    let response = request.send().await.map_err(Error::Request)?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }
    response.json().await.map_err(Error::Request)
}

fn hit_from_jackett(raw: &Value) -> Option<Hit> {
    let title = text_field(raw, &["Title", "title"]);
    if title.is_empty() {
        return None;
    }
    let magnet = text_field(raw, &["MagnetUri", "magnetUri", "Link", "link", "Guid"]);
    Some(Hit {
        title,
        tracker: text_field(raw, &["Tracker", "tracker", "TrackerId"]),
        size_bytes: row_size_bytes(raw),
        seeders: json_u32(
            raw.get("Seeders")
                .or_else(|| raw.get("seeders"))
                .unwrap_or(&Value::Null),
        ),
        peers: json_u32(
            raw.get("Peers")
                .or_else(|| raw.get("peers"))
                .or_else(|| raw.get("Leechers"))
                .unwrap_or(&Value::Null),
        ),
        magnet,
        published: format_publish_date(&text_field(
            raw,
            &["PublishDate", "publishDate", "Published"],
        )),
    })
}

async fn search_prowlarr(
    base: &str,
    api_key: &str,
    query: &SearchQuery,
    q: &str,
    use_system_proxy: bool,
) -> Result<Vec<Hit>, Error> {
    let url = join_url(base, "api/v1/search");
    let search_type = match query.kind {
        MediaKind::Tv => "tvsearch",
        _ => "search",
    };
    let client = http_client(use_system_proxy)?;
    let parsed = prowlarr_get(&client, &url, api_key, q, search_type, Some(query)).await?;
    let mut hits: Vec<Hit> = parsed.iter().filter_map(hit_from_prowlarr).collect();
    if hits.is_empty() {
        warn!(query = %q, "prowlarr empty; retrying without categories");
        let parsed = prowlarr_get(&client, &url, api_key, q, search_type, None).await?;
        hits = parsed.iter().filter_map(hit_from_prowlarr).collect();
    }
    Ok(hits)
}

async fn prowlarr_get(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    q: &str,
    search_type: &str,
    query: Option<&SearchQuery>,
) -> Result<Vec<Value>, Error> {
    let mut request = client.get(url).header("X-Api-Key", api_key).query(&[
        ("apikey", api_key),
        ("query", q),
        ("type", search_type),
    ]);
    if let Some(query) = query {
        let category = match query.kind {
            MediaKind::Tv => "5000",
            _ => "2000",
        };
        request = request.query(&[("categories", category)]);
        if query.is_anime {
            request = request.query(&[("categories", "5070")]);
        }
    }
    let response = request.send().await.map_err(Error::Request)?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }
    let body = response.bytes().await.map_err(Error::Request)?;
    serde_json::from_slice(&body).map_err(Error::BadJson)
}

fn hit_from_prowlarr(raw: &Value) -> Option<Hit> {
    let protocol = text_field(raw, &["protocol"]).to_ascii_lowercase();
    if !protocol.is_empty() && protocol != "torrent" {
        return None;
    }
    let title = text_field(raw, &["title", "Title"]);
    if title.is_empty() {
        return None;
    }
    let magnet = text_field(
        raw,
        &["magnetUrl", "MagnetUri", "downloadUrl", "guid", "Link"],
    );
    Some(Hit {
        title,
        tracker: text_field(raw, &["indexer", "Tracker"]),
        size_bytes: row_size_bytes(raw),
        seeders: json_u32(
            raw.get("seeders")
                .or_else(|| raw.get("Seeders"))
                .unwrap_or(&Value::Null),
        ),
        peers: json_u32(
            raw.get("leechers")
                .or_else(|| raw.get("Peers"))
                .unwrap_or(&Value::Null),
        ),
        magnet,
        published: format_publish_date(&text_field(
            raw,
            &["publishDate", "PublishDate", "published"],
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jackett_row_maps_magnet_or_link() {
        let raw = serde_json::json!({
            "Title": " Dune 2021 ",
            "Tracker": "rutor",
            "Size": 1234,
            "Seeders": 10,
            "Peers": "3",
            "Link": "magnet:?xt=urn:btih:ab",
            "PublishDate": "2021-10-22T12:00:00Z"
        });
        let hit = match hit_from_jackett(&raw) {
            Some(hit) => hit,
            None => panic!("hit"),
        };
        assert_eq!(hit.title, "Dune 2021");
        assert_eq!(hit.size_bytes, 1234);
        assert_eq!(hit.peers, 3);
        assert_eq!(hit.published, "22 Oct 2021");
        assert!(hit.magnet.starts_with("magnet:"));
    }

    #[test]
    fn jackett_accepts_lowercase_results_wrapper() {
        let parsed: JackettResults = match serde_json::from_str(r#"{"results":[{"title":"A"}]}"#) {
            Ok(parsed) => parsed,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(parsed.results.len(), 1);
        assert_eq!(
            hit_from_jackett(&parsed.results[0]).map(|h| h.title),
            Some(String::from("A"))
        );
    }

    #[test]
    fn sanitize_strips_lucene_colon() {
        assert_eq!(
            search_text("Spider-Man: Brand New Day"),
            "Spider-Man Brand New Day"
        );
        assert_eq!(search_text("Dune:  Part Two"), "Dune Part Two");
    }

    #[test]
    fn jackett_parses_indexer_errors() {
        let parsed: JackettResults = match serde_json::from_str(
            r#"{"Results":[],"Indexers":[{"Name":"rutor","Error":"timeout"}]}"#,
        ) {
            Ok(parsed) => parsed,
            Err(error) => panic!("{error}"),
        };
        assert!(parsed.results.is_empty());
        assert_eq!(parsed.indexers.len(), 1);
        assert_eq!(parsed.indexers[0].name, "rutor");
        assert_eq!(parsed.indexers[0].error.as_deref(), Some("timeout"));
    }

    #[test]
    fn prowlarr_skips_usenet() {
        let raw = serde_json::json!({
            "protocol": "usenet",
            "title": "Nope"
        });
        assert!(hit_from_prowlarr(&raw).is_none());
    }

    #[test]
    fn prowlarr_keeps_missing_protocol() {
        let raw = serde_json::json!({ "title": "Film", "indexer": "rutor" });
        let hit = match hit_from_prowlarr(&raw) {
            Some(hit) => hit,
            None => panic!("hit"),
        };
        assert_eq!(hit.title, "Film");
        assert_eq!(hit.tracker, "rutor");
    }

    #[test]
    fn size_parses_numeric_string_and_label() {
        let labeled = serde_json::json!({
            "Title": "A",
            "Size": "1.5 GB"
        });
        let hit = match hit_from_jackett(&labeled) {
            Some(hit) => hit,
            None => panic!("hit"),
        };
        assert_eq!(hit.size_bytes, 1_610_612_736);

        let dotted = serde_json::json!({
            "Title": "B",
            "Size": "1572864000.0"
        });
        let hit = match hit_from_jackett(&dotted) {
            Some(hit) => hit,
            None => panic!("hit"),
        };
        assert_eq!(hit.size_bytes, 1_572_864_000);
    }
}
