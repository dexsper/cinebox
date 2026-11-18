//! Per-client request limit, keyed by IP.

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::num::NonZeroU32;
use std::time::Duration;

use governor::clock::Clock;
use governor::{DefaultKeyedRateLimiter, Quota};
use http::HeaderMap;
use http::header::HeaderName;

const PRUNE_INTERVAL: Duration = Duration::from_mins(1);

/// An IPv6 client usually owns a whole /64, so the limit applies to the prefix rather than to one address of it.
const IPV6_PREFIX_SEGMENTS: usize = 4;

static X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// The limiter; without a rate it lets everything through.
#[derive(Debug)]
pub(crate) struct RateLimit {
    limiter: Option<DefaultKeyedRateLimiter<IpAddr>>,
    is_forwarded_for_trusted: bool,
}

impl RateLimit {
    /// `per_second` 0 turns the limit off; `burst` 0 means the same as `per_second`.
    #[must_use]
    pub(crate) fn new(per_second: u32, burst: u32, is_forwarded_for_trusted: bool) -> Self {
        let Some(rate) = NonZeroU32::new(per_second) else { return Self { limiter: None, is_forwarded_for_trusted } };

        let burst = NonZeroU32::new(burst).unwrap_or(rate);
        let quota = Quota::per_second(rate).allow_burst(burst);
        let limiter = DefaultKeyedRateLimiter::keyed(quota);

        Self { limiter: Some(limiter), is_forwarded_for_trusted }
    }

    #[must_use]
    pub(crate) const fn is_enabled(&self) -> bool {
        self.limiter.is_some()
    }

    /// `Err` holds how long the client should wait.
    pub(crate) fn check(&self, peer: SocketAddr, headers: &HeaderMap) -> Result<(), Duration> {
        let Some(limiter) = &self.limiter else { return Ok(()) };

        let client = self.client_ip(peer, headers);

        limiter.check_key(&client).map_err(|not_until| not_until.wait_time_from(limiter.clock().now()))
    }

    /// Drops the state of clients that are back to a full bucket, every minute, so that the table does not grow with
    /// every address ever seen.
    pub(crate) async fn prune_forever(&self) {
        let Some(limiter) = &self.limiter else { return };

        let mut ticks = tokio::time::interval(PRUNE_INTERVAL);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            ticks.tick().await;
            limiter.retain_recent();
            limiter.shrink_to_fit();
        }
    }

    fn client_ip(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
        let forwarded = self.is_forwarded_for_trusted.then(|| forwarded_for(headers)).flatten();

        normalize(forwarded.unwrap_or_else(|| peer.ip()))
    }
}

/// The left-most address of the first `X-Forwarded-For`, which is the client as the first proxy saw it.
fn forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    let value = headers.get(&X_FORWARDED_FOR)?.to_str().ok()?;
    let first = value.split(',').next()?;

    first.trim().parse().ok()
}

fn normalize(ip: IpAddr) -> IpAddr {
    let IpAddr::V6(v6) = ip else { return ip };

    if let Some(v4) = v6.to_ipv4_mapped() {
        return IpAddr::V4(v4);
    }

    let mut segments = v6.segments();

    for segment in segments.iter_mut().skip(IPV6_PREFIX_SEGMENTS) {
        *segment = 0;
    }

    IpAddr::V6(Ipv6Addr::from(segments))
}
