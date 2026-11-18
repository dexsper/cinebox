use std::sync::Arc;

use super::entry::Entry;
use super::key::Key;

// Charged on top of the body for the key, the header values and moka's own bookkeeping, so that a flood of tiny
// bodies such as cached 404s cannot hold far more memory than the budget says.
const ENTRY_OVERHEAD_BYTES: u32 = 256;

/// The in-memory tier: small entries only, weighed by body size.
#[derive(Debug)]
pub(super) struct Memory {
    entries: moka::future::Cache<Key, Arc<Entry>>,
    max_entry_bytes: u64,
}

impl Memory {
    pub(super) fn new(budget_bytes: u64, max_entry_bytes: u64) -> Self {
        let entries = moka::future::Cache::builder().max_capacity(budget_bytes).weigher(weigh).build();

        Self { entries, max_entry_bytes }
    }

    pub(super) async fn get(&self, key: &Key) -> Option<Arc<Entry>> {
        self.entries.get(key).await
    }

    #[must_use]
    pub(super) const fn admits(&self, body_len: u64) -> bool {
        body_len <= self.max_entry_bytes
    }

    /// Ignores entries larger than the per-entry limit; those are served from disk.
    pub(super) async fn insert(&self, key: Key, entry: Arc<Entry>) {
        if !self.admits(entry.head().body_len()) {
            return;
        }

        self.entries.insert(key, entry).await;
    }

    #[must_use]
    pub(super) fn weighted_size(&self) -> u64 {
        self.entries.weighted_size()
    }
}

fn weigh(_key: &Key, entry: &Arc<Entry>) -> u32 {
    let body = u32::try_from(entry.body().len()).unwrap_or(u32::MAX);

    body.saturating_add(ENTRY_OVERHEAD_BYTES)
}
