//! First-run discovery: which TorrServer and parser addresses answer right now.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cinebox_core::{KIND_CONFIG, ParserKind, ParserPreset, ServicePresets, Store, presets};
use cinebox_net::NetConfig;
use cinebox_torrserver::FoundServer;
use futures_util::future::{join, join_all};
use tracing::warn;

use crate::jobs::JobError;

const MDNS_WINDOW: Duration = Duration::from_secs(2);
const PARSER_WAIT: Duration = Duration::from_secs(5);
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const PRESETS_TTL: Duration = Duration::from_secs(24 * 3600);
const PRESETS_CACHE_ID: &str = "service-presets";

/// A parser address that answered, fastest first in [`Discovery::parsers`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundParser {
    pub kind: ParserKind,
    pub url: String,
    /// Shared server that works without an API key.
    pub public: bool,
    /// Reachable, but refused the empty key: the viewer must paste one.
    pub needs_key: bool,
    pub latency_ms: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Discovery {
    pub torrservers: Vec<FoundServer>,
    pub parsers: Vec<FoundParser>,
}

/// Where discovery reads the preset list from.
#[derive(Clone)]
pub struct DiscoveryCtx {
    pub net: NetConfig,
    pub db: Option<Arc<Store>>,
    /// Folder of `settings.json`, which may hold the user's `presets.json`.
    pub config_dir: Option<PathBuf>,
}

/// Never fails: an unreachable service is simply not listed.
pub async fn discover_services(ctx: DiscoveryCtx) -> Result<Discovery, JobError> {
    let presets = load_presets(&ctx).await;
    let torr = &presets.torrserver;

    let servers = cinebox_torrserver::discover(&torr.mdns_service, &torr.fallback, MDNS_WINDOW);
    let parsers = probe_parsers(&presets.parsers, &ctx.net);
    let (torrservers, parsers) = join(servers, parsers).await;

    Ok(Discovery {
        torrservers,
        parsers,
    })
}

async fn load_presets(ctx: &DiscoveryCtx) -> ServicePresets {
    let latest = latest_presets(ctx).await;
    let base = latest.unwrap_or_else(ServicePresets::bundled);

    let config_dir = ctx.config_dir.as_deref();
    let user = config_dir.and_then(ServicePresets::load_user);

    match user {
        Some(user) => base.merged_with(user),
        None => base,
    }
}

/// Cached copy while fresh, otherwise the published one (then cached).
async fn latest_presets(ctx: &DiscoveryCtx) -> Option<ServicePresets> {
    if let Some(cached) = cached_presets(ctx.db.as_deref()).await {
        return Some(cached);
    }

    let fetched = fetch_presets(&ctx.net).await?;
    if let Some(db) = &ctx.db {
        cache_presets(db, &fetched).await;
    }

    Some(fetched)
}

async fn cache_presets(db: &Store, presets: &ServicePresets) {
    let id = PRESETS_CACHE_ID;
    let saved = db.put_json("", KIND_CONFIG, id, presets, &[]).await;

    if let Err(error) = saved {
        warn!(%error, "failed to cache service presets");
    }
}

async fn cached_presets(db: Option<&Store>) -> Option<ServicePresets> {
    let lookup = db?.get_json("", KIND_CONFIG, PRESETS_CACHE_ID);
    let hit = lookup.await.ok()??;

    if !hit.is_fresh(PRESETS_TTL) {
        return None;
    }

    Some(hit.value)
}

async fn fetch_presets(net: &NetConfig) -> Option<ServicePresets> {
    let url = presets::presets_url();
    let request = |client: &reqwest::Client| client.get(&url);
    let sent = cinebox_net::send_resilient(net, FETCH_TIMEOUT, None, request).await;

    let response = sent.ok()?.error_for_status().ok()?;
    let body = response.text().await.ok()?;
    ServicePresets::parse(&body)
}

async fn probe_parsers(presets: &[ParserPreset], net: &NetConfig) -> Vec<FoundParser> {
    let probes = presets.iter().map(|preset| probe_parser(preset, net));
    let answers = join_all(probes).await;

    let mut found: Vec<FoundParser> = answers.into_iter().flatten().collect();
    found.sort_by_key(|parser| parser.latency_ms);
    found
}

/// Pings with an empty key: public servers answer, private ones refuse it.
async fn probe_parser(preset: &ParserPreset, net: &NetConfig) -> Option<FoundParser> {
    let started = Instant::now();
    let ping = cinebox_indexer::ping(preset.kind, &preset.url, "", net);
    let answer = tokio::time::timeout(PARSER_WAIT, ping).await.ok()?;

    let needs_key = match answer {
        Ok(_) => false,
        Err(cinebox_indexer::Error::Http(400 | 401 | 403)) => true,
        Err(_) => return None,
    };

    // A public server that wants a key is of no use to a first-time viewer.
    if needs_key && preset.public {
        return None;
    }

    let latency = started.elapsed().as_millis();
    Some(FoundParser {
        kind: preset.kind,
        url: preset.url.clone(),
        public: preset.public,
        needs_key,
        latency_ms: u64::try_from(latency).unwrap_or(u64::MAX),
    })
}
