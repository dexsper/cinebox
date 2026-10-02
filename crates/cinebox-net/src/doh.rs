//! DoH resolver on hickory-resolver: a custom URL, Quad9, DNS.SB and AliDNS,
//! asked together.
//!
//! Built-in providers carry pinned bootstrap IPs, so reaching the DoH server
//! itself never depends on (possibly poisoned) system DNS. TLS verifies the
//! real provider hostname against the bundled Mozilla roots: on Android the
//! platform verifier made the first lookup take seconds. Answer caching and
//! TTL handling live inside hickory-resolver and follow the DNS response.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinSet;
use tokio::time::Instant;

use hickory_resolver::TokioResolver;
use hickory_resolver::config::{LookupIpStrategy, NameServerConfig, ResolveHosts, ResolverConfig};
use hickory_resolver::lookup_ip::LookupIp;
use hickory_resolver::net::NetError;
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use tracing::warn;

const DOH_PATH: &str = "/dns-query";
/// After the first answer, how long the other providers may still add theirs.
const MORE_ANSWERS: Duration = Duration::from_millis(300);

struct DohProvider {
    host: &'static str,
    ips: [Ipv4Addr; 2],
}

/// Built-in RFC 8484 providers, all serving on `/dns-query` over port 443.
const PROVIDERS: [DohProvider; 3] = [
    DohProvider {
        host: "dns.quad9.net",
        ips: [Ipv4Addr::new(9, 9, 9, 9), Ipv4Addr::new(149, 112, 112, 112)],
    },
    DohProvider {
        host: "doh.dns.sb",
        ips: [
            Ipv4Addr::new(185, 222, 222, 222),
            Ipv4Addr::new(45, 11, 45, 11),
        ],
    },
    DohProvider {
        host: "dns.alidns.com",
        ips: [Ipv4Addr::new(223, 5, 5, 5), Ipv4Addr::new(223, 6, 6, 6)],
    },
];

/// `reqwest` DNS hook that resolves through every DoH provider and falls
/// back to the OS resolver when none of them answers.
///
/// A CDN answers each provider with a different edge, and an edge can be
/// blocked where another is not (TMDB's images were). Each provider has a
/// resolver of its own and their answers are merged; the connector then
/// tries the addresses in turn.
pub(crate) struct DohResolve {
    providers: Arc<[Arc<TokioResolver>]>,
}

impl DohResolve {
    /// A custom DoH URL is bootstrapped through system DNS once (its IP is
    /// unknown); failures there are logged and skipped so the built-in
    /// providers always remain available. `None` only when no resolver could
    /// be built at all.
    pub(crate) async fn new(custom_doh_url: &str) -> Option<Self> {
        let mut pools = vec![custom_name_servers(custom_doh_url).await];
        for provider in &PROVIDERS {
            pools.push(provider_name_servers(provider));
        }

        let mut providers = Vec::new();
        for servers in pools {
            if servers.is_empty() {
                continue;
            }

            if let Some(resolver) = build_resolver(servers) {
                providers.push(Arc::new(resolver));
            }
        }

        if providers.is_empty() {
            warn!("no doh resolver could be built; dns bypass disabled");
            return None;
        }

        Some(Self {
            providers: providers.into(),
        })
    }
}

fn provider_name_servers(provider: &DohProvider) -> Vec<NameServerConfig> {
    let mut servers = Vec::new();
    for ip in provider.ips {
        servers.push(name_server(IpAddr::V4(ip), provider.host, DOH_PATH));
    }

    servers
}

fn build_resolver(servers: Vec<NameServerConfig>) -> Option<TokioResolver> {
    let concurrent = servers.len().max(1);
    let config = ResolverConfig::from_name_servers(servers);
    let runtime = TokioRuntimeProvider::default();
    let mut builder = TokioResolver::builder_with_config(config, runtime);

    let opts = builder.options_mut();
    opts.timeout = Duration::from_secs(3);
    opts.attempts = 2;
    opts.ip_strategy = LookupIpStrategy::Ipv4Only;
    opts.use_hosts_file = ResolveHosts::Never;
    opts.num_concurrent_reqs = concurrent;

    match builder.build() {
        Ok(resolver) => Some(resolver),
        Err(error) => {
            warn!(%error, "doh resolver failed to build");
            None
        }
    }
}

/// Asks every provider at once and waits for the first answer, then
/// [`MORE_ANSWERS`] for the rest. Lookups still running finish in the
/// background, so the next resolve has every provider's answer cached.
async fn lookup_all(providers: &[Arc<TokioResolver>], host: &str) -> Vec<SocketAddr> {
    let mut lookups = JoinSet::new();
    for provider in providers {
        let provider = Arc::clone(provider);
        let host = host.to_owned();
        lookups.spawn(async move { provider.lookup_ip(host.as_str()).await });
    }

    let mut addrs = Vec::new();
    let mut until = None;
    while let Some(lookup) = next_lookup(&mut lookups, until).await {
        match lookup {
            Ok(found) => merge(&mut addrs, public_addrs(found)),
            Err(error) => warn!(%error, host, "doh lookup failed"),
        }

        if until.is_none() && !addrs.is_empty() {
            until = Some(Instant::now() + MORE_ANSWERS);
        }
    }

    lookups.detach_all();
    addrs
}

type Lookup = Result<LookupIp, NetError>;

/// `None` once every lookup is done or `until` has passed.
async fn next_lookup(lookups: &mut JoinSet<Lookup>, until: Option<Instant>) -> Option<Lookup> {
    loop {
        let joined = match until {
            Some(until) => tokio::time::timeout_at(until, lookups.join_next()).await.ok()?,
            None => lookups.join_next().await,
        };

        match joined? {
            Ok(lookup) => return Some(lookup),
            Err(error) => warn!(%error, "doh lookup task ended early"),
        }
    }
}

fn merge(addrs: &mut Vec<SocketAddr>, found: Vec<SocketAddr>) {
    for addr in found {
        if !addrs.contains(&addr) {
            addrs.push(addr);
        }
    }
}

fn name_server(ip: IpAddr, host: &str, path: &str) -> NameServerConfig {
    NameServerConfig::https(ip, Arc::from(host), Some(Arc::from(path)))
}

/// Parse the user DoH URL and resolve its host through system DNS once.
/// Returns an empty list (with a warning) when the URL is unusable.
async fn custom_name_servers(url: &str) -> Vec<NameServerConfig> {
    let url = url.trim();
    if url.is_empty() {
        return Vec::new();
    }

    let Ok(parsed) = reqwest::Url::parse(url) else {
        warn!(url, "custom DoH url is not a valid url; ignoring");
        return Vec::new();
    };

    let Some(host) = parsed.host_str().map(str::to_owned) else {
        warn!(url, "custom DoH url has no host; ignoring");
        return Vec::new();
    };

    let port = parsed.port().unwrap_or(443);
    let resolved = tokio::net::lookup_host((host.as_str(), port)).await;
    let addrs = match resolved {
        Ok(addrs) => addrs,
        Err(error) => {
            warn!(%error, url, "custom DoH host did not resolve; ignoring");
            return Vec::new();
        }
    };

    let path = parsed.path().to_owned();
    let servers = addrs
        .map(|addr| name_server(addr.ip(), &host, &path))
        .collect();

    servers
}

/// ISP sinkholes often map blocked names to loopback. Connecting there looks
/// like a generic TLS failure; never treat those answers as usable.
fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_multicast()
        }
        IpAddr::V6(ip) => {
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_multicast()
                && !ip.is_unicast_link_local()
        }
    }
}

fn public_addrs(ips: impl IntoIterator<Item = IpAddr>) -> Vec<SocketAddr> {
    let mut addrs = Vec::new();
    for ip in ips {
        if !is_public_ip(ip) {
            continue;
        }

        addrs.push(SocketAddr::new(ip, 0));
    }

    addrs
}

fn boxed_addrs(addrs: Vec<SocketAddr>) -> Addrs {
    Box::new(addrs.into_iter())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "hits live DoH providers"]
    async fn resolves_tmdb_via_builtin_providers() {
        let resolve = DohResolve::new("").await.expect("resolver builds");
        let addrs = lookup_all(&resolve.providers, "api.themoviedb.org.").await;

        assert!(!addrs.is_empty(), "at least one of Quad9 / DNS.SB / AliDNS answers");
    }

    #[tokio::test]
    #[ignore = "hits live TMDB over DoH"]
    async fn https_to_tmdb_via_doh_client() {
        let net = crate::NetConfig {
            use_system_proxy: false,
            dns_bypass: true,
            custom_doh_url: String::new(),
        };

        let response = crate::send_resilient(
            &net,
            Duration::from_secs(8),
            Some("cinebox-net-test"),
            |client| client.get("https://api.themoviedb.org/3"),
        )
        .await;

        match response {
            Ok(response) => {
                let status = response.status().as_u16();
                assert!(
                    (200..500).contains(&status),
                    "unexpected HTTP {status} from TMDB"
                );
            }
            Err(error) => panic!(
                "DoH HTTPS to TMDB failed: {error:?}"
            ),
        }
    }

    #[tokio::test]
    #[ignore = "hits live TMDB images over DoH"]
    async fn tmdb_images_load_via_doh_client() {
        let net = crate::NetConfig {
            use_system_proxy: false,
            dns_bypass: true,
            custom_doh_url: String::new(),
        };

        let poster = "https://image.tmdb.org/t/p/w92/wwemzKWzjKYJFfCeiB57q3r4Bcm.png";
        let response = crate::send_resilient(&net, Duration::from_secs(8), None, |client| {
            client.get(poster)
        })
        .await;

        let status = response.map(|response| response.status().as_u16());
        assert_eq!(status.ok(), Some(200), "an image edge some provider names must answer");
    }

    #[test]
    fn merged_answers_keep_the_first_order_without_repeats() {
        let a = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 0);
        let b = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(2, 2, 2, 2)), 0);
        let mut addrs = vec![a];

        merge(&mut addrs, vec![b, a]);

        assert_eq!(addrs, vec![a, b]);
    }

    #[tokio::test]
    async fn bad_custom_url_is_ignored() {
        let servers = custom_name_servers("not a url").await;
        assert!(servers.is_empty());

        let servers = custom_name_servers("").await;
        assert!(servers.is_empty());
    }

    #[test]
    fn loopback_and_unspecified_are_not_public() {
        assert!(!is_public_ip(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!is_public_ip(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        assert!(!is_public_ip(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)));
        assert!(is_public_ip(IpAddr::V4(Ipv4Addr::new(104, 16, 1, 1))));
    }

    #[test]
    fn public_addrs_drops_sinkhole() {
        let addrs = public_addrs([
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(104, 16, 1, 1)),
        ]);

        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].ip(), IpAddr::V4(Ipv4Addr::new(104, 16, 1, 1)));
    }
}

impl Resolve for DohResolve {
    fn resolve(&self, name: Name) -> Resolving {
        let providers = Arc::clone(&self.providers);

        Box::pin(async move {
            let host = name.as_str().to_owned();

            let addrs = lookup_all(&providers, &host).await;
            if !addrs.is_empty() {
                return Ok(boxed_addrs(addrs));
            }

            warn!(host, "no doh provider gave a public ip");

            // System DNS only if it yields a public IP. Poisoned answers like
            // 127.0.0.1 / ::1 are ignored so bypass cannot silently hit localhost.
            let system = match tokio::net::lookup_host((host.as_str(), 0)).await {
                Ok(system) => system,
                Err(error) => return Err(error.into()),
            };

            let addrs = public_addrs(system.map(|addr| addr.ip()));
            if addrs.is_empty() {
                return Err("dns bypass: no public ip (system dns looks poisoned)".into());
            }

            warn!(host, "using system dns after doh miss");
            Ok(boxed_addrs(addrs))
        })
    }
}
