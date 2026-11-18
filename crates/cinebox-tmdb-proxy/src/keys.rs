//! Which api key a request goes upstream with, and which client keys TMDB has already accepted.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use http::StatusCode;
use thiserror::Error;

use crate::cache::{Fetched, Mode};

const MAX_KEY_BYTES: usize = 128;

/// Bounds the memory of the known keys; each entry is a 32-byte hash plus moka's bookkeeping.
const KNOWN_KEYS_CAPACITY: u64 = 65_536;

/// The operator's TMDB key; `Debug` never shows it.
#[derive(Clone)]
pub(crate) struct OperatorKey(String);

/// blake3 of a client key, which is all the proxy keeps of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Fingerprint([u8; blake3::OUT_LEN]);

type KnownKeys = moka::sync::Cache<Fingerprint, ()>;

/// The key policy of the proxy.
#[derive(Debug)]
pub(crate) enum Keys {
    /// Clients bring their own key; only keys TMDB accepted read the shared cache without a trip upstream.
    Passthrough(KnownKeys),

    /// The operator's key goes upstream, and clients need none.
    Operator(OperatorKey),
}

/// What the keys allow for one request.
#[derive(Debug)]
pub(crate) struct Access {
    pub(crate) mode: Mode,
    pub(crate) feedback: KeyFeedback,
}

/// Learns from the upstream answer whether a client key is good; cheap to clone into a fetch.
#[derive(Debug, Clone)]
pub(crate) struct KeyFeedback(Option<(KnownKeys, Fingerprint)>);

#[derive(Debug, Error)]
pub(crate) enum KeyError {
    #[error("the TMDB key must be 1 to 128 ASCII letters and digits")]
    Malformed,

    #[error("a TMDB key is required")]
    Missing,
}

impl FromStr for OperatorKey {
    type Err = KeyError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let trimmed = text.trim();

        if !is_well_formed(trimmed) {
            return Err(KeyError::Malformed);
        }

        Ok(Self(trimmed.to_owned()))
    }
}

impl fmt::Debug for OperatorKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OperatorKey(<redacted>)")
    }
}

impl Keys {
    #[must_use]
    pub(crate) fn new(operator_key: Option<OperatorKey>, known_key_ttl: Duration) -> Self {
        if let Some(key) = operator_key {
            return Self::Operator(key);
        }

        let known = moka::sync::Cache::builder().max_capacity(KNOWN_KEYS_CAPACITY).time_to_live(known_key_ttl).build();

        Self::Passthrough(known)
    }

    #[must_use]
    pub(crate) const fn is_key_required(&self) -> bool {
        matches!(self, Self::Passthrough(_))
    }

    /// In passthrough mode a missing or malformed client key is refused; a key TMDB has not accepted yet goes upstream
    /// even when the cache holds the answer, so that the shared cache never serves an unchecked key.
    pub(crate) fn check(&self, client_key: Option<&str>) -> Result<Access, KeyError> {
        let known = match self {
            Self::Operator(_) => return Ok(Access { mode: Mode::Cached, feedback: KeyFeedback(None) }),
            Self::Passthrough(known) => known,
        };

        let key = client_key.ok_or(KeyError::Missing)?;

        if !is_well_formed(key) {
            return Err(KeyError::Malformed);
        }

        let fingerprint = Fingerprint(*blake3::hash(key.as_bytes()).as_bytes());
        let mode = if known.contains_key(&fingerprint) { Mode::Cached } else { Mode::Revalidate };
        let feedback = KeyFeedback(Some((known.clone(), fingerprint)));

        Ok(Access { mode, feedback })
    }

    /// The key to send upstream: the operator's, or the client's own.
    #[must_use]
    pub(crate) fn upstream_key<'a>(&'a self, client_key: Option<&'a str>) -> Option<&'a str> {
        match self {
            Self::Operator(OperatorKey(key)) => Some(key),
            Self::Passthrough(_) => client_key,
        }
    }
}

impl KeyFeedback {
    /// Remembers the key on a 2xx and forgets it on a 401; failures say nothing about the key.
    pub(crate) fn record(&self, fetched: &Fetched) {
        let Some((known, fingerprint)) = &self.0 else { return };

        let entry = match fetched {
            Fetched::Cacheable { entry, .. } | Fetched::Uncacheable(entry) => entry,
            Fetched::Failed(_) => return,
        };

        let status = entry.head().status();

        if status.is_success() {
            known.insert(*fingerprint, ());
            return;
        }

        if status == StatusCode::UNAUTHORIZED {
            known.invalidate(fingerprint);
        }
    }
}

fn is_well_formed(key: &str) -> bool {
    let has_valid_length = !key.is_empty() && key.len() <= MAX_KEY_BYTES;

    has_valid_length && key.bytes().all(|byte| byte.is_ascii_alphanumeric())
}
