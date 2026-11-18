use std::io;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use futures_util::Stream;
use http::{HeaderValue, StatusCode};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

const STREAM_CHUNK_BYTES: usize = 64 * 1_024;

/// How the stored body is encoded; JSON is kept gzipped, images as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    Identity,
    Gzip,
}

/// Everything about a response except its body, with the header values ready to copy into a response.
#[derive(Debug, Clone)]
pub(crate) struct Head {
    status: StatusCode,
    content_type: HeaderValue,
    content_length: HeaderValue,
    body_len: u64,
    encoding: Encoding,
    fetched_at: u64,
    expires_at: u64,
    retry_after: Option<HeaderValue>,
}

/// A response held in memory. `content_length` always matches `body`.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    head: Head,
    body: Bytes,
}

/// A large entry read from disk: the file is positioned at the start of the body.
#[derive(Debug)]
pub(crate) struct FileBody {
    head: Head,
    file: File,
}

/// How fresh an entry is at a moment, given the stale window of the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Freshness {
    Fresh,
    Stale,
    Expired,
}

impl Encoding {
    pub(super) const fn tag(self) -> u8 {
        match self {
            Self::Identity => 0,
            Self::Gzip => 1,
        }
    }

    pub(super) const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Identity),
            1 => Some(Self::Gzip),
            _ => None,
        }
    }

    /// The `Content-Encoding` value to send when the body goes out as stored; `None` for identity.
    #[must_use]
    pub(crate) const fn header_value(self) -> Option<HeaderValue> {
        match self {
            Self::Identity => None,
            Self::Gzip => Some(HeaderValue::from_static("gzip")),
        }
    }
}

impl Head {
    pub(super) fn new(status: StatusCode, content_type: HeaderValue, encoding: Encoding, body_len: u64) -> Self {
        let content_length = HeaderValue::from(body_len);

        Self {
            status,
            content_type,
            content_length,
            body_len,
            encoding,
            fetched_at: 0,
            expires_at: 0,
            retry_after: None,
        }
    }

    pub(super) const fn with_times(mut self, fetched_at: u64, expires_at: u64) -> Self {
        self.fetched_at = fetched_at;
        self.expires_at = expires_at;
        self
    }

    #[must_use]
    pub(crate) const fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    pub(crate) const fn content_type(&self) -> &HeaderValue {
        &self.content_type
    }

    /// The length of the stored body, which is the encoded length for a gzipped one.
    #[must_use]
    pub(crate) const fn content_length(&self) -> &HeaderValue {
        &self.content_length
    }

    #[must_use]
    pub(crate) const fn body_len(&self) -> u64 {
        self.body_len
    }

    #[must_use]
    pub(crate) const fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// Unix seconds; 0 for a response that was never cached.
    #[must_use]
    pub(crate) const fn fetched_at(&self) -> u64 {
        self.fetched_at
    }

    /// Unix seconds; 0 for a response that was never cached.
    #[must_use]
    pub(crate) const fn expires_at(&self) -> u64 {
        self.expires_at
    }

    /// Only set on responses that are never cached, such as a 429 passed through; not persisted.
    #[must_use]
    pub(crate) const fn retry_after(&self) -> Option<&HeaderValue> {
        self.retry_after.as_ref()
    }

    pub(super) const fn freshness(&self, now: u64, stale_window: Duration) -> Freshness {
        if now < self.expires_at {
            return Freshness::Fresh;
        }

        let stale_until = self.expires_at.saturating_add(stale_window.as_secs());

        if now < stale_until {
            return Freshness::Stale;
        }

        Freshness::Expired
    }
}

impl Entry {
    /// An uncached response; the cache stamps `fetched_at` and `expires_at` when it stores one.
    #[must_use]
    pub(crate) fn new(status: StatusCode, content_type: HeaderValue, encoding: Encoding, body: Bytes) -> Self {
        let body_len = body.len() as u64;
        let head = Head::new(status, content_type, encoding, body_len);

        Self { head, body }
    }

    #[must_use]
    pub(crate) fn with_retry_after(mut self, retry_after: HeaderValue) -> Self {
        self.head.retry_after = Some(retry_after);
        self
    }

    pub(super) const fn from_parts(head: Head, body: Bytes) -> Self {
        Self { head, body }
    }

    pub(super) fn stamped(mut self, now: u64, ttl: Duration) -> Self {
        let expires_at = now.saturating_add(ttl.as_secs());
        self.head = self.head.with_times(now, expires_at);
        self
    }

    #[must_use]
    pub(crate) const fn head(&self) -> &Head {
        &self.head
    }

    /// Cloning the returned `Bytes` only bumps a reference count.
    #[must_use]
    pub(crate) const fn body(&self) -> &Bytes {
        &self.body
    }
}

impl FileBody {
    pub(super) const fn new(head: Head, file: File) -> Self {
        Self { head, file }
    }

    #[must_use]
    pub(crate) const fn head(&self) -> &Head {
        &self.head
    }

    /// The body in chunks of up to 64 KiB. Ends with `UnexpectedEof` when the file is shorter than its header says.
    pub(crate) fn into_stream(self) -> impl Stream<Item = io::Result<Bytes>> + Send + 'static {
        let state = (self.file, self.head.body_len);

        futures_util::stream::try_unfold(state, |(mut file, remaining)| async move {
            if remaining == 0 {
                return Ok(None);
            }

            let chunk_len = usize::try_from(remaining).map_or(STREAM_CHUNK_BYTES, |left| left.min(STREAM_CHUNK_BYTES));
            let mut chunk = BytesMut::with_capacity(chunk_len);
            let read = file.read_buf(&mut chunk).await?;

            if read == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }

            let left = remaining.saturating_sub(read as u64);

            Ok(Some((chunk.freeze(), (file, left))))
        })
    }

    /// Reads the whole body, for callers that must transform it, such as gunzipping for a client without gzip.
    pub(crate) async fn read_to_bytes(mut self) -> io::Result<Bytes> {
        let capacity = usize::try_from(self.head.body_len).map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
        let mut body = BytesMut::zeroed(capacity);
        self.file.read_exact(&mut body).await?;

        Ok(body.freeze())
    }
}
