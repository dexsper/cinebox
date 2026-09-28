//! Flatpak: the sandbox sees neither the host's GSettings nor `kioslaverc`, so
//! ask the host through the `ProxyResolver` portal. It applies the desktop's
//! proxy mode, bypass list, and PAC script to each URL.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use super::SystemProxy;

#[zbus::proxy(
    interface = "org.freedesktop.portal.ProxyResolver",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait ProxyResolver {
    fn lookup(&self, uri: &str) -> zbus::Result<Vec<String>>;
}

/// Flatpak mounts this file into every sandbox.
pub(super) fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Per-request proxy for reqwest: each origin is looked up once, then cached.
pub(super) fn reqwest_proxy() -> reqwest::Proxy {
    reqwest::Proxy::custom(|url| lookup(url.scheme(), url.host_str()?, url.port_or_known_default()))
}

/// Answers for representative internet URLs, for clients that take one proxy (libmpv).
pub(super) fn system_proxy() -> Option<SystemProxy> {
    let mut proxy = SystemProxy {
        http: lookup("http", PROBE_HOST, None),
        https: lookup("https", PROBE_HOST, None),
        ..SystemProxy::default()
    };

    for field in [&mut proxy.http, &mut proxy.https] {
        if field.as_deref().is_some_and(is_socks) {
            proxy.socks = field.take();
        }
    }

    Some(proxy)
}

const PROBE_HOST: &str = "example.com";

struct Resolver {
    portal: ProxyResolverProxyBlocking<'static>,
    cache: Mutex<HashMap<String, Option<String>>>,
}

fn resolver() -> Option<&'static Resolver> {
    static RESOLVER: OnceLock<Option<Resolver>> = OnceLock::new();

    RESOLVER
        .get_or_init(|| {
            let connect = || -> zbus::Result<ProxyResolverProxyBlocking<'static>> {
                let connection = zbus::blocking::Connection::session()?;
                ProxyResolverProxyBlocking::new(&connection)
            };

            match connect() {
                Ok(portal) => Some(Resolver {
                    portal,
                    cache: Mutex::new(HashMap::new()),
                }),
                Err(error) => {
                    tracing::warn!(%error, "proxy resolver portal unavailable; connecting directly");
                    None
                }
            }
        })
        .as_ref()
}

/// Proxy URL for the origin, or `None` to connect directly.
fn lookup(scheme: &str, host: &str, port: Option<u16>) -> Option<String> {
    let resolver = resolver()?;
    let origin = match port {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    };

    let mut cache = resolver
        .cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner);

    if let Some(answer) = cache.get(&origin) {
        return answer.clone();
    }

    let answer = match resolver.portal.lookup(&origin) {
        Ok(proxies) => first_usable(&proxies),
        Err(error) => {
            tracing::warn!(%error, "proxy resolver portal lookup failed; connecting directly");
            None
        }
    };

    cache.insert(origin, answer.clone());
    answer
}

/// The portal lists proxies to try in order; `direct://` means no proxy.
fn first_usable(proxies: &[String]) -> Option<String> {
    for proxy in proxies {
        let Some((scheme, rest)) = proxy.split_once("://") else {
            continue;
        };

        match scheme {
            "direct" => return None,
            "http" | "https" | "socks4" | "socks4a" | "socks5" => return Some(proxy.clone()),
            // GLib's generic SOCKS entry; reqwest needs the version.
            "socks" => return Some(format!("socks5://{rest}")),
            _ => {}
        }
    }

    if !proxies.is_empty() {
        tracing::warn!("no supported proxy scheme in the portal answer; connecting directly");
    }

    None
}

fn is_socks(url: &str) -> bool {
    url.starts_with("socks")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(entries: &[&str]) -> Option<String> {
        let entries: Vec<String> = entries.iter().map(|entry| (*entry).to_owned()).collect();
        first_usable(&entries)
    }

    #[test]
    fn direct_means_no_proxy() {
        assert_eq!(answer(&["direct://"]), None);
    }

    #[test]
    fn http_proxy_is_used_as_is() {
        assert_eq!(
            answer(&["http://proxy:3128"]).as_deref(),
            Some("http://proxy:3128")
        );
    }

    #[test]
    fn generic_socks_becomes_socks5() {
        assert_eq!(
            answer(&["socks://proxy:1080"]).as_deref(),
            Some("socks5://proxy:1080")
        );
    }

    #[test]
    fn unsupported_entries_are_skipped_in_order() {
        assert_eq!(
            answer(&["ftp://proxy:21", "http://proxy:3128", "direct://"]).as_deref(),
            Some("http://proxy:3128")
        );
    }

    #[test]
    fn empty_answer_connects_directly() {
        assert_eq!(answer(&[]), None);
    }
}
