//! Command line and environment settings, and the values `auto` resolves to on this machine.

use std::fmt;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use cinebox_net::NetConfig;
use clap::builder::BoolishValueParser;
use clap::{ArgAction, Parser};
use thiserror::Error;

use crate::cache::CacheConfig;
use crate::keys::{KeyError, OperatorKey};
use crate::upstream::{Ttls, UpstreamConfig};

const KIBIBYTE: u64 = 1_024;
const MEBIBYTE: u64 = KIBIBYTE * KIBIBYTE;
const GIBIBYTE: u64 = MEBIBYTE * KIBIBYTE;
const TEBIBYTE: u64 = GIBIBYTE * KIBIBYTE;
const UNITS: [(&str, u64); 5] = [("B", 1), ("KiB", KIBIBYTE), ("MiB", MEBIBYTE), ("GiB", GIBIBYTE), ("TiB", TEBIBYTE)];
const SPAN_UNITS: [(&str, u64); 4] = [("d", 86_400), ("h", 3_600), ("m", 60), ("s", 1)];

const BLOCKING_THREADS_PER_CORE: usize = 4;
const MAX_BLOCKING_THREADS: usize = 64;
const CONNECTIONS_PER_CORE: usize = 256;
const MIN_CONNECTIONS: usize = 256;
const MAX_CONNECTIONS: usize = 16_384;

/// `auto` gives the memory tier this share of the machine's memory, as `1 / MEMORY_CACHE_DIVISOR`.
const MEMORY_CACHE_DIVISOR: u64 = 8;
const MIN_MEMORY_CACHE_BYTES: u64 = 16 * MEBIBYTE;
const MAX_MEMORY_CACHE_BYTES: u64 = GIBIBYTE;
const FALLBACK_MEMORY_CACHE_BYTES: u64 = 128 * MEBIBYTE;

#[cfg(target_os = "linux")]
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// Distinct keys that may wait on upstream at once, per upstream slot; beyond it a miss gets a fast 503.
const FLIGHTS_PER_UPSTREAM_SLOT: usize = 16;

/// The command line; every flag can also come from the environment variable named next to it.
#[derive(Debug, Parser)]
#[command(name = "cinebox-tmdb-proxy", version, about = "Caching reverse proxy for the TMDB API and images")]
pub(crate) struct Args {
    /// Address to listen on, plain HTTP; TLS belongs to the reverse proxy in front.
    #[arg(long, env = "LISTEN", default_value = "127.0.0.1:8080")]
    listen: SocketAddr,

    /// External address of this proxy that TMDB links in responses are rewritten to [default: `http://{listen}`].
    #[arg(long, env = "PUBLIC_URL", value_parser = parse_public_url)]
    public_url: Option<String>,

    /// Directory of the disk cache.
    #[arg(long, env = "CACHE_DIR", default_value = "./cache")]
    cache_dir: PathBuf,

    /// Disk cache budget, clamped to 80% of the free space plus what the cache already holds.
    #[arg(long = "disk-cache", value_name = "SIZE", env = "DISK_CACHE", default_value = "5GiB", value_parser = parse_size)]
    disk_cache_bytes: u64,

    /// Tokio worker threads [default: number of cores].
    #[arg(long, env = "WORKERS")]
    workers: Option<NonZeroUsize>,

    /// Threads for file I/O [default: 4 per core, at most 64].
    #[arg(long, env = "BLOCKING_THREADS")]
    blocking_threads: Option<NonZeroUsize>,

    /// Open client connections; `auto` is 256 per core, from 256 to 16384.
    #[arg(long, env = "MAX_CONNECTIONS", default_value = "auto", value_parser = parse_auto_count)]
    max_connections: Auto<NonZeroUsize>,

    /// Memory cache budget; `auto` is 1/8 of the memory, from 16 MiB to 1 GiB.
    #[arg(long = "memory-cache", value_name = "SIZE", env = "MEMORY_CACHE", default_value = "auto", value_parser = parse_auto_size)]
    memory_cache_bytes: Auto<u64>,

    /// Larger entries skip the memory cache and are streamed from disk.
    #[arg(long = "memory-max-entry", value_name = "SIZE", env = "MEMORY_MAX_ENTRY", default_value = "64KiB", value_parser = parse_size)]
    memory_max_entry_bytes: u64,

    /// Larger upstream bodies are served but not cached.
    #[arg(long = "max-body", value_name = "SIZE", env = "MAX_BODY", default_value = "32MiB", value_parser = parse_size)]
    max_body_bytes: u64,

    /// Requests to TMDB at once.
    #[arg(long, env = "UPSTREAM_CONCURRENCY", default_value = "20")]
    upstream_concurrency: NonZeroUsize,

    /// Limit for one request to TMDB, including the wait for a free slot.
    #[arg(long, env = "UPSTREAM_TIMEOUT", default_value = "20s", value_parser = humantime::parse_duration)]
    upstream_timeout: Duration,

    /// Requests per second per client IP; 0 turns the limit off.
    #[arg(long, env = "RATE_LIMIT", default_value_t = 50)]
    rate_limit: u32,

    /// Requests a client IP may send at once above the rate.
    #[arg(long, env = "RATE_BURST", default_value_t = 100)]
    rate_burst: u32,

    /// Take the client IP from the left-most `X-Forwarded-For`; only behind a reverse proxy that sets it.
    #[arg(long, env = "TRUST_FORWARDED_FOR", default_value_t = false, action = ArgAction::Set, num_args = 0..=1,
        default_missing_value = "true", value_parser = BoolishValueParser::new())]
    trust_forwarded_for: bool,

    /// Operator mode: send this TMDB key upstream and let clients go without one; empty means passthrough mode.
    #[arg(long, env = "TMDB_API_KEY", hide_env_values = true, value_parser = parse_tmdb_key)]
    tmdb_key: Option<KeyArg>,

    /// Freshness of `configuration`.
    #[arg(long, env = "TTL_CONFIG", default_value = "7d", value_parser = humantime::parse_duration)]
    ttl_config: Duration,

    /// Freshness of cards, seasons, people and collections.
    #[arg(long, env = "TTL_DETAILS", default_value = "24h", value_parser = humantime::parse_duration)]
    ttl_details: Duration,

    /// Freshness of search and discover.
    #[arg(long, env = "TTL_SEARCH", default_value = "6h", value_parser = humantime::parse_duration)]
    ttl_search: Duration,

    /// Freshness of `now_playing`, `popular`, `trending` and other lists.
    #[arg(long, env = "TTL_LISTS", default_value = "3h", value_parser = humantime::parse_duration)]
    ttl_lists: Duration,

    /// Freshness of images.
    #[arg(long, env = "TTL_IMAGES", default_value = "365d", value_parser = humantime::parse_duration)]
    ttl_images: Duration,

    /// Freshness of a 404 from TMDB.
    #[arg(long, env = "TTL_NOT_FOUND", default_value = "10m", value_parser = humantime::parse_duration)]
    ttl_not_found: Duration,

    /// How long past expiry an entry is still served while TMDB is down.
    #[arg(long, env = "STALE_IF_ERROR", default_value = "30d", value_parser = humantime::parse_duration)]
    stale_if_error: Duration,

    /// How long a key TMDB accepted is trusted to read the shared cache.
    #[arg(long, env = "KNOWN_KEY_TTL", default_value = "24h", value_parser = humantime::parse_duration)]
    known_key_ttl: Duration,

    /// Resolve TMDB hosts over DoH, around DNS blocking.
    #[arg(long, env = "DNS_BYPASS", default_value_t = true, action = ArgAction::Set, num_args = 0..=1,
        default_missing_value = "true", value_parser = BoolishValueParser::new())]
    dns_bypass: bool,

    /// DoH endpoint tried before the built-in ones.
    #[arg(long, env = "DOH_URL", default_value = "")]
    doh_url: String,

    /// Reach TMDB through the system proxy first.
    #[arg(long, env = "SYSTEM_PROXY", default_value_t = false, action = ArgAction::Set, num_args = 0..=1,
        default_missing_value = "true", value_parser = BoolishValueParser::new())]
    system_proxy: bool,

    /// Interval of the cache statistics line in the log.
    #[arg(long, env = "STATS_INTERVAL", default_value = "10m", value_parser = parse_interval)]
    stats_interval: Duration,
}

/// A value given as `auto` or as a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Auto<T> {
    Auto,
    Fixed(T),
}

/// `--tmdb-key` as given; `None` for a blank value, as an empty `TMDB_API_KEY=` in an env file leaves it.
#[derive(Debug, Clone)]
struct KeyArg(Option<OperatorKey>);

/// The settings in effect, with every `auto` resolved.
#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) listen: SocketAddr,

    /// The public url was not given and points at the listen address, which clients elsewhere cannot reach.
    pub(crate) is_public_url_guessed: bool,

    pub(crate) workers: usize,
    pub(crate) blocking_threads: usize,
    pub(crate) max_connections: usize,
    pub(crate) cache: CacheConfig,
    pub(crate) upstream: UpstreamConfig,
    pub(crate) rate_limit: u32,
    pub(crate) rate_burst: u32,
    pub(crate) trust_forwarded_for: bool,
    pub(crate) operator_key: Option<OperatorKey>,
    pub(crate) known_key_ttl: Duration,
    pub(crate) stats_interval: Duration,
}

/// Formats a byte count in the largest binary unit that divides it.
struct ByteSize(u64);

/// Formats a duration in whole seconds, in the largest unit that divides it, as the flags take it: `365d`, `20s`.
struct Span(Duration);

#[derive(Debug, Error)]
enum ArgError {
    #[error("{0:?} is not a size such as 512MiB or 5GiB")]
    Size(String),

    #[error("{0:?} is not a count or `auto`")]
    Count(String),

    #[error("{0:?} must start with http:// or https:// and hold no spaces, quotes or backslashes")]
    PublicUrl(String),

    #[error("{0:?} is not a duration above zero, such as 10m")]
    Interval(String),
}

impl Config {
    #[must_use]
    pub(crate) fn resolve(args: Args) -> Self {
        let cores = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let workers = args.workers.map_or(cores, NonZeroUsize::get);
        let default_blocking = cores.saturating_mul(BLOCKING_THREADS_PER_CORE).min(MAX_BLOCKING_THREADS);
        let blocking_threads = args.blocking_threads.map_or(default_blocking, NonZeroUsize::get);

        let max_connections = match args.max_connections {
            Auto::Auto => cores.saturating_mul(CONNECTIONS_PER_CORE).clamp(MIN_CONNECTIONS, MAX_CONNECTIONS),
            Auto::Fixed(count) => count.get(),
        };

        let memory_budget_bytes = match args.memory_cache_bytes {
            Auto::Auto => auto_memory_cache_bytes(),
            Auto::Fixed(bytes) => bytes,
        };

        let upstream_concurrency = args.upstream_concurrency.get();

        let cache = CacheConfig {
            cache_dir: args.cache_dir,
            disk_budget_bytes: args.disk_cache_bytes,
            memory_budget_bytes,
            memory_max_entry_bytes: args.memory_max_entry_bytes,
            stale_if_error: args.stale_if_error,
            max_in_flight: upstream_concurrency.saturating_mul(FLIGHTS_PER_UPSTREAM_SLOT),
        };

        let is_public_url_guessed = args.public_url.is_none();
        let public_url = args.public_url.unwrap_or_else(|| format!("http://{}", args.listen));

        let ttls = Ttls {
            config: args.ttl_config,
            details: args.ttl_details,
            search: args.ttl_search,
            lists: args.ttl_lists,
            images: args.ttl_images,
            not_found: args.ttl_not_found,
        };

        let net = NetConfig {
            use_system_proxy: args.system_proxy,
            dns_bypass: args.dns_bypass,
            custom_doh_url: args.doh_url,
        };

        let upstream = UpstreamConfig {
            net,
            timeout: args.upstream_timeout,
            concurrency: upstream_concurrency,
            max_body_bytes: args.max_body_bytes,
            public_url,
            ttls,
        };

        Self {
            listen: args.listen,
            is_public_url_guessed,
            workers,
            blocking_threads,
            max_connections,
            cache,
            upstream,
            rate_limit: args.rate_limit,
            rate_burst: args.rate_burst,
            trust_forwarded_for: args.trust_forwarded_for,
            operator_key: args.tmdb_key.and_then(|key| key.0),
            known_key_ttl: args.known_key_ttl,
            stats_interval: args.stats_interval,
        }
    }

    /// One line with every setting in effect; the operator key is only named by its mode.
    pub(crate) fn log(&self) {
        let key_mode = if self.operator_key.is_some() { "operator" } else { "passthrough" };
        let ttls = &self.upstream.ttls;
        let net = &self.upstream.net;

        tracing::info!(
            listen = %self.listen,
            public_url = %self.upstream.public_url,
            cache_dir = %self.cache.cache_dir.display(),
            disk_cache = %ByteSize(self.cache.disk_budget_bytes),
            memory_cache = %ByteSize(self.cache.memory_budget_bytes),
            memory_max_entry = %ByteSize(self.cache.memory_max_entry_bytes),
            max_body = %ByteSize(self.upstream.max_body_bytes),
            workers = self.workers,
            blocking_threads = self.blocking_threads,
            max_connections = self.max_connections,
            upstream_concurrency = self.upstream.concurrency,
            upstream_timeout = %Span(self.upstream.timeout),
            rate_limit = self.rate_limit,
            rate_burst = self.rate_burst,
            trust_forwarded_for = self.trust_forwarded_for,
            key_mode,
            ttl_config = %Span(ttls.config),
            ttl_details = %Span(ttls.details),
            ttl_search = %Span(ttls.search),
            ttl_lists = %Span(ttls.lists),
            ttl_images = %Span(ttls.images),
            ttl_not_found = %Span(ttls.not_found),
            stale_if_error = %Span(self.cache.stale_if_error),
            known_key_ttl = %Span(self.known_key_ttl),
            dns_bypass = net.dns_bypass,
            doh_url = %net.custom_doh_url,
            system_proxy = net.use_system_proxy,
            stats_interval = %Span(self.stats_interval),
            "effective configuration"
        );

        if self.is_public_url_guessed {
            tracing::warn!(
                public_url = %self.upstream.public_url,
                "--public-url is not set; TMDB links in responses point at the listen address"
            );
        }
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0;
        let mut units = UNITS.iter().rev();
        let fitting = units.find(|(_, multiplier)| bytes >= *multiplier && bytes.is_multiple_of(*multiplier));
        let (name, multiplier) = fitting.copied().unwrap_or(("B", 1));

        write!(formatter, "{}{name}", bytes / multiplier)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let seconds = self.0.as_secs();
        let mut units = SPAN_UNITS.iter();
        let fitting = units.find(|(_, multiplier)| seconds >= *multiplier && seconds.is_multiple_of(*multiplier));
        let (name, multiplier) = fitting.copied().unwrap_or(("s", 1));

        write!(formatter, "{}{name}", seconds / multiplier)
    }
}

/// Accepts a plain byte count or a number with a binary unit, case-insensitive: `512MiB`, `5GiB`, `64kib`.
fn parse_size(text: &str) -> Result<u64, ArgError> {
    let trimmed = text.trim();
    let digits_end = trimmed.find(|symbol: char| !symbol.is_ascii_digit()).unwrap_or(trimmed.len());
    let (digits, unit) = trimmed.split_at(digits_end);
    let unit = match unit.trim() {
        "" => "B",
        unit => unit,
    };

    let invalid = || ArgError::Size(text.to_owned());
    let number: u64 = digits.parse().map_err(|_| invalid())?;
    let found = UNITS.iter().find(|(name, _)| name.eq_ignore_ascii_case(unit));
    let &(_, multiplier) = found.ok_or_else(invalid)?;

    number.checked_mul(multiplier).ok_or_else(invalid)
}

fn parse_auto_size(text: &str) -> Result<Auto<u64>, ArgError> {
    if text.trim().eq_ignore_ascii_case("auto") {
        return Ok(Auto::Auto);
    }

    parse_size(text).map(Auto::Fixed)
}

fn parse_auto_count(text: &str) -> Result<Auto<NonZeroUsize>, ArgError> {
    let trimmed = text.trim();

    if trimmed.eq_ignore_ascii_case("auto") {
        return Ok(Auto::Auto);
    }

    trimmed.parse().map(Auto::Fixed).map_err(|_| ArgError::Count(text.to_owned()))
}

fn parse_public_url(text: &str) -> Result<String, ArgError> {
    let trimmed = text.trim().trim_end_matches('/');
    let has_scheme = trimmed.starts_with("http://") || trimmed.starts_with("https://");
    // The url is pasted into JSON bodies as is, so anything that would need escaping there is refused.
    let is_plain = trimmed.bytes().all(|byte| byte.is_ascii_graphic() && byte != b'"' && byte != b'\\');

    if !has_scheme || !is_plain {
        return Err(ArgError::PublicUrl(text.to_owned()));
    }

    Ok(trimmed.to_owned())
}

fn parse_tmdb_key(text: &str) -> Result<KeyArg, KeyError> {
    if text.trim().is_empty() {
        return Ok(KeyArg(None));
    }

    text.parse().map(|key| KeyArg(Some(key)))
}

fn parse_interval(text: &str) -> Result<Duration, ArgError> {
    let interval = humantime::parse_duration(text).map_err(|_| ArgError::Interval(text.to_owned()))?;

    if interval.is_zero() {
        return Err(ArgError::Interval(text.to_owned()));
    }

    Ok(interval)
}

/// A share of the machine's memory, rounded down to whole MiB.
fn auto_memory_cache_bytes() -> u64 {
    let Some(total_bytes) = detect_memory_bytes() else { return FALLBACK_MEMORY_CACHE_BYTES };

    let share = (total_bytes / MEMORY_CACHE_DIVISOR).clamp(MIN_MEMORY_CACHE_BYTES, MAX_MEMORY_CACHE_BYTES);

    share - (share % MEBIBYTE)
}

/// The smaller of the physical memory and the cgroup v2 limits, as Docker `mem_limit` and systemd `MemoryMax=` set them.
#[cfg(target_os = "linux")]
fn detect_memory_bytes() -> Option<u64> {
    let total = std::fs::read_to_string("/proc/meminfo").ok().as_deref().and_then(parse_mem_total);
    let limit = cgroup_memory_limit();

    match (total, limit) {
        (Some(total), Some(limit)) => Some(total.min(limit)),
        (total, limit) => total.or(limit),
    }
}

/// The lowest `memory.max` of the process's own cgroup and its ancestors; a limit on a parent slice binds as well.
#[cfg(target_os = "linux")]
fn cgroup_memory_limit() -> Option<u64> {
    let membership = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    let own_path = membership.lines().find_map(|line| line.strip_prefix("0::")).unwrap_or("/");
    let root = std::path::Path::new(CGROUP_ROOT);
    let relative = std::path::Path::new(own_path.trim_start_matches('/'));
    // Outside the cgroup namespace of its creator the path climbs with `..`, which would leave the mount.
    let is_inside = relative.components().all(|component| matches!(component, std::path::Component::Normal(_)));
    let own_dir = if is_inside { root.join(relative) } else { root.to_path_buf() };

    let dirs = own_dir.ancestors().take_while(|dir| dir.starts_with(root));
    let limits = dirs.filter_map(|dir| std::fs::read_to_string(dir.join("memory.max")).ok());

    limits.filter_map(|memory_max| parse_cgroup_limit(&memory_max)).min()
}

#[cfg(not(target_os = "linux"))]
const fn detect_memory_bytes() -> Option<u64> {
    None
}

/// Reads `MemTotal:  16337492 kB`.
#[cfg(target_os = "linux")]
fn parse_mem_total(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find_map(|line| line.strip_prefix("MemTotal:"))?;
    let kibibytes: u64 = line.trim().trim_end_matches("kB").trim().parse().ok()?;

    kibibytes.checked_mul(KIBIBYTE)
}

/// `None` for `max`, which means no limit.
#[cfg(target_os = "linux")]
fn parse_cgroup_limit(memory_max: &str) -> Option<u64> {
    memory_max.trim().parse().ok()
}
