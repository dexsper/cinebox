//! Unified outgoing-HTTP layer for every cinebox crate.
//!
//! One place decides how a request leaves the machine: through the system
//! proxy, directly, or — when the primary path fails and the DNS bypass is
//! enabled — directly with hostnames resolved over DoH (Quad9 / DNS.SB /
//! AliDNS), sidestepping ISP-level DNS blocking of TMDB.

#![forbid(unsafe_code)]

mod client;
mod doh;

pub use client::{plain_client, send_resilient};

/// Snapshot of the network settings shared by every outgoing HTTP client.
///
/// Doubles as the cache key for the long-lived `reqwest` clients.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct NetConfig {
    /// WinINet / env HTTP(S) proxy for the primary connection path.
    pub use_system_proxy: bool,
    /// Resolve hosts over DoH when the primary path cannot reach the server.
    pub dns_bypass: bool,
    /// Optional user DoH endpoint, tried before the built-in providers.
    pub custom_doh_url: String,
}

impl NetConfig {
    /// Direct connection: no proxy, no DoH (the TorrServer policy).
    #[must_use]
    pub fn direct() -> Self {
        Self::default()
    }
}
