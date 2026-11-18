//! Routes, the whitelist that keeps the proxy from being an open one, and responses built from cache entries.

use std::io::{self, Read};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::response::Response;
use axum::routing::get;
use bytes::Bytes;
use flate2::read::GzDecoder;
use http::header::{ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, RETRY_AFTER, VARY};
use http::request::Parts;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};

use crate::cache::{Cache, CacheError, Encoding, Fetched, Head, Key, Mode, Payload, Route, Served};
use crate::keys::{Access, KeyFeedback, Keys};
use crate::limit::RateLimit;
use crate::server::PeerAddr;
use crate::upstream::{Upstream, UpstreamRequest};

const API_PREFIX: &str = "/3/";
const IMAGE_PREFIX: &str = "/t/p/";
const API_KEY_PARAM: &str = "api_key";
const MAX_PATH_BYTES: usize = 2 * 1_024;
const MAX_QUERY_BYTES: usize = 2 * 1_024;
const MAX_IMAGE_SIZE_DIGITS: usize = 5;
const IMAGE_EXTENSIONS: [&str; 4] = ["jpg", "png", "webp", "svg"];
const RESPONSE_HEADER_CAPACITY: usize = 8;

/// gzip bodies of TMDB JSON usually shrink to a fifth or less, so the plain body is sized for that up front.
const GUNZIP_CAPACITY_FACTOR: usize = 5;

const NOT_FOUND_BODY: &str = r#"{"error":"not found"}"#;
const QUERY_TOO_LONG_BODY: &str = r#"{"error":"query too long"}"#;
const KEY_REQUIRED_BODY: &str = r#"{"error":"a valid api_key is required"}"#;
const RATE_LIMITED_BODY: &str = r#"{"error":"too many requests"}"#;
const BUSY_BODY: &str = r#"{"error":"busy, retry shortly"}"#;
const FETCH_ABORTED_BODY: &str = r#"{"error":"upstream fetch failed"}"#;
const INTERNAL_ERROR_BODY: &str = r#"{"error":"internal error"}"#;
const HEALTHY_BODY: &str = r#"{"status":"ok"}"#;

static X_CACHE: HeaderName = HeaderName::from_static("x-cache");
static ACCEPT_ENCODING_VALUE: HeaderValue = HeaderValue::from_static("accept-encoding");
static NO_STORE: HeaderValue = HeaderValue::from_static("no-store");
static JSON_TYPE: HeaderValue = HeaderValue::from_static("application/json;charset=utf-8");
static RETRY_SOON: HeaderValue = HeaderValue::from_static("1");

/// Everything the handlers share.
#[derive(Debug)]
pub(crate) struct App {
    cache: Cache,
    upstream: Upstream,
    keys: Keys,
    limit: Arc<RateLimit>,
    info: Bytes,
}

/// How the client wants the body.
#[derive(Debug, Clone, Copy)]
struct Delivery {
    route: Route,
    is_head: bool,
    accepts_gzip: bool,
}

impl App {
    #[must_use]
    pub(crate) fn new(cache: Cache, upstream: Upstream, keys: Keys, limit: Arc<RateLimit>) -> Self {
        let version = env!("CARGO_PKG_VERSION");
        let key_required = keys.is_key_required();
        let info = format!(r#"{{"service":"cinebox-tmdb-proxy","version":"{version}","key_required":{key_required}}}"#);

        Self { cache, upstream, keys, limit, info: Bytes::from(info) }
    }

    async fn fetch_api(&self, uri: &Uri, feedback: KeyFeedback) -> Fetched {
        let path = uri.path().strip_prefix(API_PREFIX).unwrap_or_default();
        let query = uri.query();
        let api_key = self.keys.upstream_key(query.and_then(api_key_of));

        let fetched = self.upstream.fetch(UpstreamRequest { route: Route::Api, path, query, api_key }).await;
        feedback.record(&fetched);

        fetched
    }

    async fn fetch_image(&self, uri: &Uri) -> Fetched {
        let path = uri.path().strip_prefix(IMAGE_PREFIX).unwrap_or_default();

        self.upstream.fetch(UpstreamRequest { route: Route::Image, path, query: None, api_key: None }).await
    }

    async fn respond(&self, served: Result<Served, CacheError>, delivery: Delivery) -> Response {
        let served = match served {
            Ok(served) => served,
            Err(CacheError::Busy) => {
                return with_retry_after(error_response(StatusCode::SERVICE_UNAVAILABLE, BUSY_BODY));
            }
            Err(CacheError::Aborted) => return error_response(StatusCode::BAD_GATEWAY, FETCH_ABORTED_BODY),
        };

        let head = served.payload.head().clone();
        let mut headers = HeaderMap::with_capacity(RESPONSE_HEADER_CAPACITY);
        headers.insert(CONTENT_TYPE, head.content_type().clone());
        headers.insert(CACHE_CONTROL, cache_control(&head, delivery.route));
        headers.insert(VARY, ACCEPT_ENCODING_VALUE.clone());
        headers.insert(&X_CACHE, served.status.header_value());

        if let Some(retry_after) = head.retry_after() {
            headers.insert(RETRY_AFTER, retry_after.clone());
        }

        let needs_gunzip = head.encoding() == Encoding::Gzip && !delivery.accepts_gzip;

        if needs_gunzip {
            return self.respond_gunzipped(head.status(), headers, served.payload, delivery.is_head).await;
        }

        if let Some(encoding) = head.encoding().header_value() {
            headers.insert(CONTENT_ENCODING, encoding);
        }

        headers.insert(CONTENT_LENGTH, head.content_length().clone());

        if delivery.is_head {
            return build_response(head.status(), headers, Body::empty());
        }

        self.cache.record_served(head.body_len());

        build_response(head.status(), headers, stored_body(served.payload))
    }

    async fn respond_gunzipped(
        &self,
        status: StatusCode,
        mut headers: HeaderMap,
        payload: Payload,
        is_head: bool,
    ) -> Response {
        let plain = match gunzip(payload).await {
            Ok(plain) => plain,
            Err(error) => {
                tracing::warn!(%error, "cannot gunzip a cached body");
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_ERROR_BODY);
            }
        };

        headers.insert(CONTENT_LENGTH, HeaderValue::from(plain.len()));

        if is_head {
            return build_response(status, headers, Body::empty());
        }

        self.cache.record_served(plain.len() as u64);

        build_response(status, headers, Body::from(plain))
    }
}

/// The router. Non-GET methods and anything outside the whitelist get a short JSON 404.
pub(crate) fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/3/{*path}", get(api).fallback(not_found))
        .route("/t/p/{size}/{file}", get(image).fallback(not_found))
        .route("/info", get(info).fallback(not_found))
        .route("/healthz", get(healthz).fallback(not_found))
        .fallback(not_found)
        .with_state(app)
}

async fn api(State(app): State<Arc<App>>, request: Request) -> Response {
    let (parts, _body) = request.into_parts();
    let Some(path) = parts.uri.path().strip_prefix(API_PREFIX) else { return not_found().await };

    if !is_valid_api_path(path) {
        return not_found().await;
    }

    let query = parts.uri.query();

    if query.is_some_and(|query| query.len() > MAX_QUERY_BYTES) {
        return error_response(StatusCode::URI_TOO_LONG, QUERY_TOO_LONG_BODY);
    }

    if let Err(wait) = app.limit.check(peer_of(&parts), &parts.headers) {
        return rate_limited(wait);
    }

    let Ok(Access { mode, feedback }) = app.keys.check(query.and_then(api_key_of)) else {
        return error_response(StatusCode::UNAUTHORIZED, KEY_REQUIRED_BODY);
    };

    let pairs = query.unwrap_or_default().split('&').filter(|pair| !pair.is_empty());
    let key = Key::new(Route::Api, path, pairs.map(split_pair));

    let fetch = {
        let app = Arc::clone(&app);
        let uri = parts.uri.clone();
        move || async move { app.fetch_api(&uri, feedback).await }
    };

    let served = app.cache.get_or_fetch(key, mode, fetch).await;

    app.respond(served, delivery_of(&parts, Route::Api)).await
}

async fn image(State(app): State<Arc<App>>, request: Request) -> Response {
    let (parts, _body) = request.into_parts();
    let Some(path) = parts.uri.path().strip_prefix(IMAGE_PREFIX) else { return not_found().await };

    if !is_valid_image_path(path) {
        return not_found().await;
    }

    if let Err(wait) = app.limit.check(peer_of(&parts), &parts.headers) {
        return rate_limited(wait);
    }

    // Images need no key upstream, and the query is never forwarded, so it stays out of the key as well.
    let key = Key::new(Route::Image, path, std::iter::empty());

    let fetch = {
        let app = Arc::clone(&app);
        let uri = parts.uri.clone();
        move || async move { app.fetch_image(&uri).await }
    };

    let served = app.cache.get_or_fetch(key, Mode::Cached, fetch).await;

    app.respond(served, delivery_of(&parts, Route::Image)).await
}

async fn info(State(app): State<Arc<App>>) -> Response {
    json_response(StatusCode::OK, Body::from(app.info.clone()))
}

async fn healthz() -> Response {
    json_response(StatusCode::OK, Body::from(HEALTHY_BODY))
}

async fn not_found() -> Response {
    error_response(StatusCode::NOT_FOUND, NOT_FOUND_BODY)
}

fn error_response(status: StatusCode, body: &'static str) -> Response {
    json_response(status, Body::from(body))
}

/// A JSON answer of the proxy itself, never cached by clients.
fn json_response(status: StatusCode, body: Body) -> Response {
    let mut headers = HeaderMap::with_capacity(RESPONSE_HEADER_CAPACITY);
    headers.insert(CONTENT_TYPE, JSON_TYPE.clone());
    headers.insert(CACHE_CONTROL, NO_STORE.clone());

    build_response(status, headers, body)
}

fn with_retry_after(mut response: Response) -> Response {
    response.headers_mut().insert(RETRY_AFTER, RETRY_SOON.clone());

    response
}

fn rate_limited(wait: Duration) -> Response {
    let has_fraction = wait.subsec_nanos() > 0;
    let seconds = wait.as_secs().saturating_add(u64::from(has_fraction)).max(1);

    let mut response = error_response(StatusCode::TOO_MANY_REQUESTS, RATE_LIMITED_BODY);
    response.headers_mut().insert(RETRY_AFTER, HeaderValue::from(seconds));

    response
}

fn build_response(status: StatusCode, headers: HeaderMap, body: Body) -> Response {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;

    response
}

/// A cached entry has `public, max-age` of what is left of its freshness, and a fresh image is also `immutable`.
fn cache_control(head: &Head, route: Route) -> HeaderValue {
    if head.expires_at() == 0 {
        return NO_STORE.clone();
    }

    let remaining = head.expires_at().saturating_sub(unix_now());
    let is_immutable = route == Route::Image && head.status() == StatusCode::OK && remaining > 0;
    let immutable = if is_immutable { ", immutable" } else { "" };
    let value = format!("public, max-age={remaining}{immutable}");

    HeaderValue::try_from(value).unwrap_or_else(|_| NO_STORE.clone())
}

/// A memory body goes out without a copy, a file body as a stream.
fn stored_body(payload: Payload) -> Body {
    match payload {
        Payload::Memory(entry) => Body::from(entry.body().clone()),
        Payload::File(file) => Body::from_stream((*file).into_stream()),
    }
}

async fn gunzip(payload: Payload) -> io::Result<Bytes> {
    let compressed = match payload {
        Payload::Memory(entry) => entry.body().clone(),
        Payload::File(file) => (*file).read_to_bytes().await?,
    };

    let mut plain = Vec::with_capacity(compressed.len().saturating_mul(GUNZIP_CAPACITY_FACTOR));
    GzDecoder::new(compressed.as_ref()).read_to_end(&mut plain)?;

    Ok(Bytes::from(plain))
}

fn delivery_of(parts: &Parts, route: Route) -> Delivery {
    Delivery { route, is_head: parts.method == Method::HEAD, accepts_gzip: accepts_gzip(&parts.headers) }
}

fn peer_of(parts: &Parts) -> SocketAddr {
    let unknown = SocketAddr::from(([0, 0, 0, 0], 0));

    parts.extensions.get::<PeerAddr>().map_or(unknown, |peer| peer.0)
}

fn accepts_gzip(headers: &HeaderMap) -> bool {
    let values = headers.get_all(ACCEPT_ENCODING).iter().filter_map(|value| value.to_str().ok());
    let mut codings = values.flat_map(|value| value.split(','));

    codings.any(is_gzip_coding)
}

/// `gzip` or `*`, unless refused with `q=0`.
fn is_gzip_coding(coding: &str) -> bool {
    let mut parts = coding.split(';');
    let name = parts.next().unwrap_or_default().trim();

    if !name.eq_ignore_ascii_case("gzip") && name != "*" {
        return false;
    }

    !parts.any(is_zero_quality)
}

fn is_zero_quality(param: &str) -> bool {
    let Some((name, value)) = param.split_once('=') else { return false };

    name.trim().eq_ignore_ascii_case("q") && value.trim().parse::<f32>().is_ok_and(|quality| quality <= 0.0)
}

fn api_key_of(query: &str) -> Option<&str> {
    let mut pairs = query.split('&').map(split_pair);

    pairs.find_map(|(name, value)| (name == API_KEY_PARAM).then_some(value))
}

fn split_pair(pair: &str) -> (&str, &str) {
    pair.split_once('=').unwrap_or((pair, ""))
}

/// Segments of `[A-Za-z0-9_.-]`, none empty, `.` or holding `..`.
fn is_valid_api_path(path: &str) -> bool {
    path.len() <= MAX_PATH_BYTES && path.split('/').all(is_valid_segment)
}

fn is_valid_segment(segment: &str) -> bool {
    let is_plain = !segment.is_empty() && segment.bytes().all(is_segment_byte);

    is_plain && segment != "." && !segment.contains("..")
}

const fn is_segment_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')
}

/// `{size}/{file}`: size `w123`, `h123` or `original`, file `[A-Za-z0-9]+.(jpg|png|webp|svg)`.
fn is_valid_image_path(path: &str) -> bool {
    if path.len() > MAX_PATH_BYTES {
        return false;
    }

    let Some((size, file)) = path.split_once('/') else { return false };

    is_valid_image_size(size) && is_valid_image_file(file)
}

fn is_valid_image_size(size: &str) -> bool {
    if size == "original" {
        return true;
    }

    let Some(digits) = size.strip_prefix(['w', 'h']) else { return false };

    let has_valid_length = (1..=MAX_IMAGE_SIZE_DIGITS).contains(&digits.len());

    has_valid_length && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_valid_image_file(file: &str) -> bool {
    let Some((stem, extension)) = file.split_once('.') else { return false };

    let is_plain_stem = !stem.is_empty() && stem.bytes().all(|byte| byte.is_ascii_alphanumeric());

    is_plain_stem && IMAGE_EXTENSIONS.contains(&extension)
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}
