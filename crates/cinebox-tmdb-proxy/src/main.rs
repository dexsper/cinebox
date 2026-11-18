//! Caching reverse proxy for TMDB: `/3/*` to the API, `/t/p/*` to images.

mod cache;
mod config;
mod keys;
mod limit;
mod routes;
mod server;
mod upstream;

use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tracing_subscriber::EnvFilter;

use crate::cache::Cache;
use crate::config::{Args, Config};
use crate::keys::Keys;
use crate::limit::RateLimit;
use crate::routes::App;
use crate::upstream::Upstream;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    init_tracing();

    let config = Config::resolve(args);
    config.log();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(config.workers)
        .max_blocking_threads(config.blocking_threads)
        .enable_all()
        .build()
        .context("cannot start the async runtime")?;

    runtime.block_on(run(config))
}

async fn run(config: Config) -> anyhow::Result<()> {
    let cache_dir = config.cache.cache_dir.display().to_string();
    let cache = Cache::open(config.cache).await.with_context(|| format!("cannot open the cache in {cache_dir}"))?;
    let listen = config.listen;
    let listener = TcpListener::bind(listen).await.with_context(|| format!("cannot listen on {listen}"))?;

    let limit = Arc::new(RateLimit::new(config.rate_limit, config.rate_burst, config.trust_forwarded_for));
    let keys = Keys::new(config.operator_key, config.known_key_ttl);
    let upstream = Upstream::new(config.upstream);
    let app = Arc::new(App::new(cache.clone(), upstream, keys, Arc::clone(&limit)));

    let mut background = JoinSet::new();
    background.spawn(log_stats(cache, config.stats_interval));

    if limit.is_enabled() {
        background.spawn(async move { limit.prune_forever().await });
    }

    tracing::info!(listen = %config.listen, "listening");
    server::serve(listener, routes::router(app), config.max_connections, server::shutdown_signal()).await;

    background.shutdown().await;
    tracing::info!("stopped");

    Ok(())
}

/// `RUST_LOG` picks the level, `info` when unset. Colors only on a terminal, so that the journal and log files stay
/// plain.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let is_terminal = std::io::stdout().is_terminal();

    tracing_subscriber::fmt().with_env_filter(filter).with_ansi(is_terminal).init();
}

/// One line of cache totals per interval; no addresses, paths or keys.
async fn log_stats(cache: Cache, interval: Duration) {
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticks.tick().await;

    loop {
        ticks.tick().await;
        let stats = cache.stats();

        tracing::info!(
            hits = stats.hits,
            misses = stats.misses,
            stale = stats.stale,
            served_bytes = stats.served_bytes,
            memory_bytes = stats.memory_bytes,
            disk_bytes = stats.disk_bytes,
            disk_entries = stats.disk_entries,
            "cache stats"
        );
    }
}
