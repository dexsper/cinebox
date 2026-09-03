//! Jackett JSON and Prowlarr REST search (HTTP).

use std::time::Duration;

use cinebox_core::{MediaKind, ParserKind, join_url, normalize_base_url};
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::map::{Hit, hit_from_jackett, hit_from_prowlarr};
use crate::query::{SearchQuery, search_text};
use crate::{Error, http_client};

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

const SEARCH_TIMEOUT: Duration = Duration::from_secs(45);

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
    let mut request = client
        .get(url)
        .timeout(SEARCH_TIMEOUT)
        .query(&[("apikey", api_key), ("Query", q)]);
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
    let mut request = client
        .get(url)
        .timeout(SEARCH_TIMEOUT)
        .header("X-Api-Key", api_key)
        .query(&[("apikey", api_key), ("query", q), ("type", search_type)]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::hit_from_jackett;

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
}
