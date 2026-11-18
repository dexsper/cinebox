use std::sync::atomic::{AtomicU64, Ordering};

/// Counters bumped on the hot path. Each is read on its own for a periodic log line, so no ordering between them is
/// needed and `Relaxed` is enough.
#[derive(Debug, Default)]
pub(super) struct Stats {
    hits: AtomicU64,
    misses: AtomicU64,
    stale: AtomicU64,
    served_bytes: AtomicU64,
}

/// A point-in-time copy of the counters and of the size of both tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct StatsSnapshot {
    pub(crate) hits: u64,
    pub(crate) misses: u64,
    pub(crate) stale: u64,
    pub(crate) served_bytes: u64,
    pub(crate) memory_bytes: u64,
    pub(crate) disk_bytes: u64,
    pub(crate) disk_entries: u64,
}

/// Sizes of the tiers, which moka already tracks, so the cache does not count them twice.
#[derive(Debug, Clone, Copy)]
pub(super) struct TierSizes {
    pub(super) memory_bytes: u64,
    pub(super) disk_bytes: u64,
    pub(super) disk_entries: u64,
}

impl Stats {
    pub(super) fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn record_stale(&self) {
        self.stale.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn record_served(&self, bytes: u64) {
        self.served_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub(super) fn snapshot(&self, tiers: TierSizes) -> StatsSnapshot {
        StatsSnapshot {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            stale: self.stale.load(Ordering::Relaxed),
            served_bytes: self.served_bytes.load(Ordering::Relaxed),
            memory_bytes: tiers.memory_bytes,
            disk_bytes: tiers.disk_bytes,
            disk_entries: tiers.disk_entries,
        }
    }
}
