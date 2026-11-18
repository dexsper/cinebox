//! Two-tier response cache: small entries in memory, everything on disk, with single-flight misses,
//! stale-while-revalidate and stale-if-error.

mod disk;
mod entry;
mod key;
mod memory;
mod stats;

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use http::HeaderValue;
use thiserror::Error;
use tokio::sync::watch;

use disk::{Disk, Loaded};
use entry::Freshness;
pub(crate) use entry::{Encoding, Entry, FileBody, Head};
pub(crate) use key::{Key, Route};
use memory::Memory;
pub(crate) use stats::StatsSnapshot;
use stats::{Stats, TierSizes};

type Flight = watch::Receiver<Option<Arc<Entry>>>;

/// Settings of the cache; sizes are already resolved from `auto`.
#[derive(Debug, Clone)]
pub(crate) struct CacheConfig {
    pub(crate) cache_dir: PathBuf,

    /// Clamped at startup to 80% of the free space on the volume of `cache_dir`.
    pub(crate) disk_budget_bytes: u64,

    pub(crate) memory_budget_bytes: u64,

    /// Larger entries skip memory and are streamed from disk.
    pub(crate) memory_max_entry_bytes: u64,

    /// How long past expiry an entry may still be served, while it is refreshed or while upstream fails.
    pub(crate) stale_if_error: Duration,

    /// Distinct keys fetched at once; a miss beyond it gets `CacheError::Busy`.
    pub(crate) max_in_flight: usize,
}

/// The cache, cheap to clone and shared by all handlers.
#[derive(Debug, Clone)]
pub(crate) struct Cache {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    memory: Memory,
    disk: Arc<Disk>,
    flights: Mutex<HashMap<Key, Flight>>,
    max_in_flight: usize,
    stale_window: Duration,
    stats: Stats,
}

/// How `Cache::get_or_fetch` treats what it already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Serve fresh or stale entries; a miss joins the one fetch in flight for the key.
    Cached,

    /// Always fetch with this caller's own request, never shared with others, as for an api key not yet seen.
    /// The result replaces the entry when cacheable; when the fetch fails, an entry within the stale window is
    /// served instead.
    Revalidate,
}

/// What the fetch closure made of the upstream response.
#[derive(Debug)]
pub(crate) enum Fetched {
    /// Store for `ttl` and serve: a 200, or a 404 with a short `ttl`.
    Cacheable { entry: Entry, ttl: Duration },

    /// Serve, but never store and never let it replace a stored entry: 401, 429, oversized bodies.
    Uncacheable(Entry),

    /// Upstream is down or answered 5xx; the entry is the error response to give when nothing stale is at hand.
    Failed(Entry),
}

/// The value for `X-Cache`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheStatus {
    Hit,
    Miss,
    Stale,
}

/// A response body ready to send.
#[derive(Debug)]
pub(crate) enum Payload {
    Memory(Arc<Entry>),
    File(Box<FileBody>),
}

/// A response and where it came from.
#[derive(Debug)]
pub(crate) struct Served {
    pub(crate) status: CacheStatus,
    pub(crate) payload: Payload,
}

#[derive(Debug, Error)]
pub(crate) enum CacheError {
    #[error("too many distinct keys are being fetched at once")]
    Busy,

    #[error("the fetch for this key stopped without a result")]
    Aborted,
}

// Removes the key from the flights when the leading task ends, including by a panic, so that the next miss starts over.
struct FlightGuard {
    inner: Arc<Inner>,
    key: Key,
}

impl Cache {
    /// Must be called inside a Tokio runtime: it spawns the disk index walk and the file deleter.
    pub(crate) async fn open(config: CacheConfig) -> io::Result<Self> {
        let disk = Disk::open(config.cache_dir, config.disk_budget_bytes, config.memory_max_entry_bytes).await?;
        let memory = Memory::new(config.memory_budget_bytes, config.memory_max_entry_bytes);

        let inner = Inner {
            memory,
            disk,
            flights: Mutex::new(HashMap::new()),
            max_in_flight: config.max_in_flight,
            stale_window: config.stale_if_error,
            stats: Stats::default(),
        };

        Ok(Self { inner: Arc::new(inner) })
    }

    /// Serves `key` from memory, then disk, then `fetch`. `fetch` runs at most once, and may run in a background task
    /// after this call returns, which is why it must be `'static`.
    pub(crate) async fn get_or_fetch<F, Fut>(&self, key: Key, mode: Mode, fetch: F) -> Result<Served, CacheError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Fetched> + Send + 'static,
    {
        let served = match mode {
            Mode::Cached => self.serve_cached(key, fetch).await?,
            Mode::Revalidate => self.revalidate(key, fetch).await,
        };

        self.record(served.status);

        Ok(served)
    }

    /// Counts bytes actually written to clients, which only the server knows (HEAD, gunzip).
    pub(crate) fn record_served(&self, bytes: u64) {
        self.inner.stats.record_served(bytes);
    }

    #[must_use]
    pub(crate) fn stats(&self) -> StatsSnapshot {
        let tiers = TierSizes {
            memory_bytes: self.inner.memory.weighted_size(),
            disk_bytes: self.inner.disk.weighted_size(),
            disk_entries: self.inner.disk.entry_count(),
        };

        self.inner.stats.snapshot(tiers)
    }

    async fn serve_cached<F, Fut>(&self, key: Key, fetch: F) -> Result<Served, CacheError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Fetched> + Send + 'static,
    {
        if let Some(payload) = self.find(key).await {
            match payload.head().freshness(unix_now(), self.inner.stale_window) {
                Freshness::Fresh => return Ok(Served { status: CacheStatus::Hit, payload }),
                Freshness::Stale => {
                    // At most one refresh per key: a refresh already in flight is joined, and none starts when busy.
                    drop(self.join_flight(key, fetch));
                    return Ok(Served { status: CacheStatus::Stale, payload });
                }
                Freshness::Expired => {}
            }
        }

        let flight = self.join_flight(key, fetch).ok_or(CacheError::Busy)?;
        let entry = wait_for(flight).await?;

        Ok(Served { status: CacheStatus::Miss, payload: Payload::Memory(entry) })
    }

    async fn revalidate<F, Fut>(&self, key: Key, fetch: F) -> Served
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Fetched> + Send + 'static,
    {
        let fetched = fetch().await;

        let failed = match fetched {
            Fetched::Cacheable { entry, ttl } => {
                let entry = self.admit(key, entry, ttl).await;
                tokio::spawn(store_on_disk(Arc::clone(&self.inner.disk), key, Arc::clone(&entry)));
                return Served::miss(entry);
            }
            Fetched::Uncacheable(entry) => return Served::miss(Arc::new(entry)),
            Fetched::Failed(entry) => entry,
        };

        let Some(payload) = self.find(key).await else { return Served::miss(Arc::new(failed)) };

        if payload.head().freshness(unix_now(), self.inner.stale_window) == Freshness::Expired {
            return Served::miss(Arc::new(failed));
        }

        Served { status: CacheStatus::Stale, payload }
    }

    async fn find(&self, key: Key) -> Option<Payload> {
        if let Some(entry) = self.inner.memory.get(&key).await {
            return Some(Payload::Memory(entry));
        }

        match self.inner.disk.load(key).await? {
            Loaded::Inline(entry) => {
                let entry = Arc::new(entry);
                self.inner.memory.insert(key, Arc::clone(&entry)).await;
                Some(Payload::Memory(entry))
            }
            Loaded::Large(body) => Some(Payload::File(Box::new(body))),
        }
    }

    /// `None` when `max_in_flight` keys are already being fetched and none of them is `key`.
    fn join_flight<F, Fut>(&self, key: Key, fetch: F) -> Option<Flight>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Fetched> + Send + 'static,
    {
        let mut flights = self.inner.lock_flights();

        if let Some(flight) = flights.get(&key) {
            return Some(flight.clone());
        }

        if flights.len() >= self.inner.max_in_flight {
            return None;
        }

        let (sender, flight) = watch::channel(None);
        flights.insert(key, flight.clone());
        drop(flights);

        tokio::spawn(self.clone().lead(key, fetch, sender));

        Some(flight)
    }

    async fn lead<F, Fut>(self, key: Key, fetch: F, sender: watch::Sender<Option<Arc<Entry>>>)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Fetched> + Send + 'static,
    {
        let _guard = FlightGuard { inner: Arc::clone(&self.inner), key };

        let entry = match fetch().await {
            Fetched::Cacheable { entry, ttl } => self.admit(key, entry, ttl).await,
            Fetched::Uncacheable(entry) | Fetched::Failed(entry) => {
                sender.send_replace(Some(Arc::new(entry)));
                return;
            }
        };

        // Waiters wake now; the flight stays joinable until the file is written, so that a miss arriving meanwhile
        // takes this result instead of fetching again.
        sender.send_replace(Some(Arc::clone(&entry)));
        self.inner.disk.store(key, entry).await;
    }

    async fn admit(&self, key: Key, entry: Entry, ttl: Duration) -> Arc<Entry> {
        let entry = Arc::new(entry.stamped(unix_now(), ttl));
        self.inner.memory.insert(key, Arc::clone(&entry)).await;

        entry
    }

    fn record(&self, status: CacheStatus) {
        match status {
            CacheStatus::Hit => self.inner.stats.record_hit(),
            CacheStatus::Miss => self.inner.stats.record_miss(),
            CacheStatus::Stale => self.inner.stats.record_stale(),
        }
    }
}

impl Inner {
    // The map only gains or loses one receiver per locked section, so it stays consistent after a panic elsewhere.
    fn lock_flights(&self) -> MutexGuard<'_, HashMap<Key, Flight>> {
        self.flights.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl CacheStatus {
    #[must_use]
    pub(crate) const fn header_value(self) -> HeaderValue {
        match self {
            Self::Hit => HeaderValue::from_static("HIT"),
            Self::Miss => HeaderValue::from_static("MISS"),
            Self::Stale => HeaderValue::from_static("STALE"),
        }
    }
}

impl Payload {
    #[must_use]
    pub(crate) fn head(&self) -> &Head {
        match self {
            Self::Memory(entry) => entry.head(),
            Self::File(body) => body.head(),
        }
    }
}

impl Served {
    const fn miss(entry: Arc<Entry>) -> Self {
        Self { status: CacheStatus::Miss, payload: Payload::Memory(entry) }
    }
}

impl Drop for FlightGuard {
    fn drop(&mut self) {
        self.inner.lock_flights().remove(&self.key);
    }
}

async fn wait_for(mut flight: Flight) -> Result<Arc<Entry>, CacheError> {
    let landed = flight.wait_for(Option::is_some).await.map_err(|_closed| CacheError::Aborted)?;
    let entry = landed.as_ref().map(Arc::clone);

    entry.ok_or(CacheError::Aborted)
}

async fn store_on_disk(disk: Arc<Disk>, key: Key, entry: Arc<Entry>) {
    disk.store(key, entry).await;
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}
