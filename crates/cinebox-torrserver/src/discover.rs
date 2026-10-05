//! Find TorrServer on the local network over mDNS / DNS-SD (Bonjour).

use std::net::IpAddr;
use std::time::Duration;

use futures_util::future::join_all;
use mdns_sd::{ResolvedService, ScopedIp, ServiceDaemon, ServiceEvent};
use tokio::time::{Instant, timeout_at};

use crate::probe::echo;
use crate::server::Server;

/// A TorrServer that answered `/echo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundServer {
    pub url: String,
    pub version: String,
}

/// Listen for `service` announcements for `window`, then confirm every
/// address with `/echo`. `fallback` addresses are checked when nothing is
/// announced (mDNS blocked, or TorrServer built without it).
pub async fn discover(service: &str, fallback: &[String], window: Duration) -> Vec<FoundServer> {
    let mut urls = browse(service, window).await;
    if urls.is_empty() {
        urls = fallback.to_vec();
    }

    confirm(urls).await
}

async fn browse(service: &str, window: Duration) -> Vec<String> {
    let Ok(daemon) = ServiceDaemon::new() else {
        return Vec::new();
    };

    let urls = collect_announced(&daemon, service, window).await;
    let _ = daemon.shutdown();
    urls
}

async fn collect_announced(daemon: &ServiceDaemon, service: &str, window: Duration) -> Vec<String> {
    let Ok(events) = daemon.browse(service) else {
        return Vec::new();
    };

    let deadline = Instant::now() + window;
    let mut urls = Vec::new();
    while let Ok(Ok(event)) = timeout_at(deadline, events.recv_async()).await {
        let ServiceEvent::ServiceResolved(found) = event else {
            continue;
        };

        let Some(url) = service_url(&found) else {
            continue;
        };

        if !urls.contains(&url) {
            urls.push(url);
        }
    }

    urls
}

fn service_url(service: &ResolvedService) -> Option<String> {
    let port = service.port;
    let url = match preferred_ip(service)? {
        IpAddr::V4(ip) => format!("http://{ip}:{port}"),
        IpAddr::V6(ip) => format!("http://[{ip}]:{port}"),
    };

    Some(url)
}

/// IPv4 first: a link-local IPv6 address needs a scope id that URLs carry poorly.
fn preferred_ip(service: &ResolvedService) -> Option<IpAddr> {
    let ips: Vec<IpAddr> = service.addresses.iter().map(ScopedIp::to_ip_addr).collect();
    let v4 = ips.iter().find(|ip| ip.is_ipv4());

    v4.or_else(|| ips.first()).copied()
}

async fn confirm(urls: Vec<String>) -> Vec<FoundServer> {
    let checks = urls.into_iter().map(check);
    let answers = join_all(checks).await;

    answers.into_iter().flatten().collect()
}

async fn check(url: String) -> Option<FoundServer> {
    let server = Server {
        url,
        ..Server::default()
    };
    let version = echo(&server).await.ok()?;

    Some(FoundServer {
        url: server.url,
        version,
    })
}
