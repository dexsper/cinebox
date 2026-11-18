//! Requests to TMDB, and turning the answers into cache entries that hold no TMDB address.

use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use cinebox_net::NetConfig;
use flate2::Compression;
use flate2::write::GzEncoder;
use http::header::{CONTENT_TYPE, RETRY_AFTER};
use http::{HeaderValue, StatusCode};
use memchr::memmem::Finder;
use tokio::sync::Semaphore;

use crate::cache::{Encoding, Entry, Fetched, Route};

const API_BASE: &str = "https://api.themoviedb.org/3";
const IMAGE_BASE: &str = "https://image.tmdb.org/t/p";
const API_KEY_PARAM: &str = "api_key";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A body over `--max-body` is still read and served up to this many times the limit, then dropped as an error.
const OVERSIZE_READ_FACTOR: u64 = 4;

/// List endpoints that change through the day, matched against the last path segment.
const LIST_ENDPOINTS: [&str; 6] = ["now_playing", "popular", "top_rated", "upcoming", "airing_today", "on_the_air"];

const UPSTREAM_ERROR_BODY: &str = r#"{"error":"upstream error"}"#;
const UPSTREAM_NOT_FOUND_BODY: &str = r#"{"error":"not found"}"#;
const UPSTREAM_BUSY_BODY: &str = r#"{"error":"upstream busy"}"#;
const UPSTREAM_TIMEOUT_BODY: &str = r#"{"error":"upstream timed out"}"#;
const UPSTREAM_UNREACHABLE_BODY: &str = r#"{"error":"upstream unreachable"}"#;

/// Freshness of each class of response.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ttls {
    pub(crate) config: Duration,
    pub(crate) details: Duration,
    pub(crate) search: Duration,
    pub(crate) lists: Duration,
    pub(crate) images: Duration,
    pub(crate) not_found: Duration,
}

#[derive(Debug, Clone)]
pub(crate) struct UpstreamConfig {
    pub(crate) net: NetConfig,

    /// For one request, including the wait for a free slot.
    pub(crate) timeout: Duration,

    pub(crate) concurrency: usize,
    pub(crate) max_body_bytes: u64,

    /// Without a trailing slash; TMDB addresses in JSON bodies are rewritten to it.
    pub(crate) public_url: String,

    pub(crate) ttls: Ttls,
}

/// The TMDB client of the proxy, shared by all requests.
#[derive(Debug)]
pub(crate) struct Upstream {
    net: NetConfig,
    timeout: Duration,
    slots: Semaphore,
    max_body_bytes: u64,
    rewriter: Rewriter,
    ttls: Ttls,
}

/// One validated request: `path` has no leading slash, `query` is raw and may hold the client's `api_key`, which is
/// replaced by `api_key` here.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UpstreamRequest<'a> {
    pub(crate) route: Route,
    pub(crate) path: &'a str,
    pub(crate) query: Option<&'a str>,
    pub(crate) api_key: Option<&'a str>,
}

/// Replaces TMDB addresses in JSON bodies with the public url of the proxy.
#[derive(Debug)]
struct Rewriter {
    rules: Vec<Rule>,
}

#[derive(Debug)]
struct Rule {
    finder: Finder<'static>,
    replacement: Vec<u8>,
}

enum ReadBody {
    Complete { body: Bytes, is_oversized: bool },
    TooLarge,
}

/// An error with every cause behind it, since reqwest's own message rarely says what went wrong.
struct Causes<'a>(&'a dyn Error);

impl Upstream {
    #[must_use]
    pub(crate) fn new(config: UpstreamConfig) -> Self {
        Self {
            net: config.net,
            timeout: config.timeout,
            slots: Semaphore::new(config.concurrency),
            max_body_bytes: config.max_body_bytes,
            rewriter: Rewriter::new(&config.public_url),
            ttls: config.ttls,
        }
    }

    /// Never forwards an upstream header other than `Content-Type` and `Retry-After`, and never a non-JSON error body,
    /// so that no TMDB address reaches the client.
    pub(crate) async fn fetch(&self, request: UpstreamRequest<'_>) -> Fetched {
        let Ok(Ok(_slot)) = tokio::time::timeout(self.timeout, self.slots.acquire()).await else {
            return Fetched::Failed(error_entry(StatusCode::SERVICE_UNAVAILABLE, UPSTREAM_BUSY_BODY));
        };

        let url = upstream_url(request);
        let connect_timeout = self.timeout.min(CONNECT_TIMEOUT);
        let build = |client: &reqwest::Client| client.get(&url).timeout(self.timeout);

        let response = match cinebox_net::send_resilient(&self.net, connect_timeout, None, build).await {
            Ok(response) => response,
            Err(error) => return Fetched::Failed(transport_failure(error)),
        };

        let status = response.status();
        let content_type = response.headers().get(CONTENT_TYPE).cloned();
        let retry_after = response.headers().get(RETRY_AFTER).cloned();

        let (body, is_oversized) = match self.read_body(response).await {
            Ok(ReadBody::Complete { body, is_oversized }) => (body, is_oversized),
            Ok(ReadBody::TooLarge) => {
                tracing::warn!(status = status.as_u16(), "upstream body is too large to serve");
                return Fetched::Failed(error_entry(StatusCode::BAD_GATEWAY, UPSTREAM_ERROR_BODY));
            }
            Err(error) => return Fetched::Failed(transport_failure(error)),
        };

        if status.is_server_error() {
            tracing::warn!(status = status.as_u16(), "upstream answered with a server error");
            return Fetched::Failed(error_entry(status, UPSTREAM_ERROR_BODY));
        }

        let entry = self.entry(status, content_type, body);

        if is_oversized {
            return Fetched::Uncacheable(entry);
        }

        match status {
            StatusCode::OK => Fetched::Cacheable { entry, ttl: self.ttl(request) },
            StatusCode::NOT_FOUND => Fetched::Cacheable { entry, ttl: self.ttls.not_found },
            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = retry_after.unwrap_or_else(|| HeaderValue::from_static("1"));
                Fetched::Uncacheable(entry.with_retry_after(retry_after))
            }
            _ => Fetched::Uncacheable(entry),
        }
    }

    async fn read_body(&self, mut response: reqwest::Response) -> Result<ReadBody, reqwest::Error> {
        let hard_limit = self.max_body_bytes.saturating_mul(OVERSIZE_READ_FACTOR);
        let mut body = BytesMut::new();

        while let Some(chunk) = response.chunk().await? {
            let total = (body.len() as u64).saturating_add(chunk.len() as u64);

            if total > hard_limit {
                return Ok(ReadBody::TooLarge);
            }

            body.extend_from_slice(&chunk);
        }

        let is_oversized = body.len() as u64 > self.max_body_bytes;

        Ok(ReadBody::Complete { body: body.freeze(), is_oversized })
    }

    /// JSON is rewritten and gzipped once here; a 200 of another type passes as is, and any other body is replaced,
    /// since an HTML error page may carry TMDB addresses.
    fn entry(&self, status: StatusCode, content_type: Option<HeaderValue>, body: Bytes) -> Entry {
        let is_json = content_type.as_ref().is_some_and(is_json_type);

        if is_json {
            let content_type = content_type.unwrap_or_else(json_type);
            return self.json_entry(status, content_type, body);
        }

        if status == StatusCode::NOT_FOUND {
            return error_entry(status, UPSTREAM_NOT_FOUND_BODY);
        }

        if status != StatusCode::OK {
            return error_entry(status, UPSTREAM_ERROR_BODY);
        }

        let content_type = content_type.unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));

        Entry::new(status, content_type, Encoding::Identity, body)
    }

    fn json_entry(&self, status: StatusCode, content_type: HeaderValue, body: Bytes) -> Entry {
        let plain = self.rewriter.rewrite(&body).map_or(body, Bytes::from);

        match gzip(&plain) {
            Ok(compressed) => Entry::new(status, content_type, Encoding::Gzip, Bytes::from(compressed)),
            Err(error) => {
                tracing::warn!(%error, "cannot gzip a response, storing it uncompressed");
                Entry::new(status, content_type, Encoding::Identity, plain)
            }
        }
    }

    fn ttl(&self, request: UpstreamRequest<'_>) -> Duration {
        let ttls = &self.ttls;

        if request.route == Route::Image {
            return ttls.images;
        }

        let first = request.path.split('/').next().unwrap_or_default();
        let last = request.path.rsplit('/').next().unwrap_or_default();

        match first {
            "configuration" => ttls.config,
            "search" | "discover" => ttls.search,
            "trending" => ttls.lists,
            _ if LIST_ENDPOINTS.contains(&last) => ttls.lists,
            _ => ttls.details,
        }
    }
}

impl Rewriter {
    fn new(public_url: &str) -> Self {
        let images = format!("{public_url}/t/p").into_bytes();
        let api = format!("{public_url}/3").into_bytes();

        let rules = vec![
            Rule::new(IMAGE_BASE, images.clone()),
            Rule::new("http://image.tmdb.org/t/p", images),
            Rule::new(API_BASE, api),
        ];

        Self { rules }
    }

    /// `None` when the body holds no TMDB address, which is the common case and costs no copy.
    fn rewrite(&self, body: &[u8]) -> Option<Vec<u8>> {
        let mut rewritten: Option<Vec<u8>> = None;

        for rule in &self.rules {
            let source = rewritten.as_deref().unwrap_or(body);

            if let Some(replaced) = rule.replace_all(source) {
                rewritten = Some(replaced);
            }
        }

        rewritten
    }
}

impl Rule {
    fn new(needle: &str, replacement: Vec<u8>) -> Self {
        Self { finder: Finder::new(needle).into_owned(), replacement }
    }

    fn replace_all(&self, haystack: &[u8]) -> Option<Vec<u8>> {
        let mut matches = self.finder.find_iter(haystack).peekable();
        matches.peek()?;

        let needle_len = self.finder.needle().len();
        let mut replaced = Vec::with_capacity(haystack.len());
        let mut copied_to = 0;

        for start in matches {
            replaced.extend_from_slice(&haystack[copied_to..start]);
            replaced.extend_from_slice(&self.replacement);
            copied_to = start + needle_len;
        }

        replaced.extend_from_slice(&haystack[copied_to..]);

        Some(replaced)
    }
}

impl fmt::Display for Causes<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)?;

        let mut source = self.0.source();

        while let Some(cause) = source {
            write!(formatter, ": {cause}")?;
            source = cause.source();
        }

        Ok(())
    }
}

/// A short JSON error made by the proxy itself; never cached on its own, so it has no times.
#[must_use]
pub(crate) fn error_entry(status: StatusCode, body: &'static str) -> Entry {
    Entry::new(status, json_type(), Encoding::Identity, Bytes::from_static(body.as_bytes()))
}

fn json_type() -> HeaderValue {
    HeaderValue::from_static("application/json;charset=utf-8")
}

fn is_json_type(content_type: &HeaderValue) -> bool {
    let prefix = b"application/json";
    let head = content_type.as_bytes().get(..prefix.len());

    head.is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn upstream_url(request: UpstreamRequest<'_>) -> String {
    let base = match request.route {
        Route::Api => API_BASE,
        Route::Image => IMAGE_BASE,
    };

    let query_len = request.query.map_or(0, str::len);
    let key_len = request.api_key.map_or(0, |key| key.len() + API_KEY_PARAM.len());
    let capacity = base.len() + request.path.len() + query_len + key_len + "/?=".len();
    let mut url = String::with_capacity(capacity);
    url.push_str(base);
    url.push('/');
    url.push_str(request.path);

    if request.route == Route::Image {
        return url;
    }

    let pairs = request.query.unwrap_or_default().split('&');
    let mut separator = '?';

    for pair in pairs.filter(|pair| !pair.is_empty() && !is_api_key_pair(pair)) {
        url.push(separator);
        url.push_str(pair);
        separator = '&';
    }

    if let Some(key) = request.api_key {
        url.push(separator);
        url.push_str(API_KEY_PARAM);
        url.push('=');
        url.push_str(key);
    }

    url
}

fn is_api_key_pair(pair: &str) -> bool {
    let name = pair.split_once('=').map_or(pair, |(name, _)| name);

    name == API_KEY_PARAM
}

/// Logs without the url, which carries the api key.
fn transport_failure(error: reqwest::Error) -> Entry {
    let is_timeout = error.is_timeout();
    let error = error.without_url();
    tracing::warn!(error = %Causes(&error), "upstream request failed");

    if is_timeout {
        return error_entry(StatusCode::GATEWAY_TIMEOUT, UPSTREAM_TIMEOUT_BODY);
    }

    error_entry(StatusCode::BAD_GATEWAY, UPSTREAM_UNREACHABLE_BODY)
}

fn gzip(body: &[u8]) -> io::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::with_capacity(body.len() / 4), Compression::default());
    encoder.write_all(body)?;

    encoder.finish()
}
