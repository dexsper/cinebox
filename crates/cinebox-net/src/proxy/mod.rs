//! System proxy discovery: environment variables first, then the OS or desktop settings.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "windows")]
mod windows;

use crate::NetConfig;

/// Proxy endpoints configured for the current user, as URLs with a scheme.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SystemProxy {
    pub(crate) http: Option<String>,
    pub(crate) https: Option<String>,
    pub(crate) socks: Option<String>,
    /// Hosts, domains, and CIDR ranges that go direct.
    pub(crate) bypass: Vec<String>,
}

impl SystemProxy {
    /// One URL for clients that take a single proxy (libmpv): HTTPS, then HTTP, then SOCKS.
    fn single_url(&self) -> Option<String> {
        self.https
            .clone()
            .or_else(|| self.http.clone())
            .or_else(|| self.socks.clone())
    }

    fn is_empty(&self) -> bool {
        self.http.is_none() && self.https.is_none() && self.socks.is_none()
    }

    /// reqwest picks the first proxy that matches a request, so SOCKS goes last as the catch-all.
    #[cfg(target_os = "linux")]
    fn apply(&self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        let bypass = reqwest::NoProxy::from_string(&self.bypass.join(","));
        let builder = add_proxy(builder, self.http.as_deref(), &bypass, |url: &str| {
            reqwest::Proxy::http(url)
        });
        let builder = add_proxy(builder, self.https.as_deref(), &bypass, |url: &str| {
            reqwest::Proxy::https(url)
        });

        add_proxy(builder, self.socks.as_deref(), &bypass, |url: &str| {
            reqwest::Proxy::all(url)
        })
    }
}

#[cfg(target_os = "linux")]
fn add_proxy(
    builder: reqwest::ClientBuilder,
    url: Option<&str>,
    bypass: &Option<reqwest::NoProxy>,
    make: impl Fn(&str) -> reqwest::Result<reqwest::Proxy>,
) -> reqwest::ClientBuilder {
    let Some(url) = url else {
        return builder;
    };

    match make(url) {
        Ok(proxy) => builder.proxy(proxy.no_proxy(bypass.clone())),
        Err(error) => {
            tracing::warn!(%error, "ignoring an invalid system proxy entry");
            builder
        }
    }
}

/// Proxy URL for stream downloads when [`NetConfig::use_system_proxy`] is on.
///
/// Reads `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY`, then the OS or desktop
/// settings. PAC scripts are not evaluated. Do not log the return value: it may
/// contain credentials.
#[must_use]
pub fn http_proxy_url(net: &NetConfig) -> Option<String> {
    if !net.use_system_proxy {
        return None;
    }

    env_http_proxy().or_else(|| platform_proxy()?.single_url())
}

/// reqwest reads the environment itself but not GNOME or KDE settings, so pass
/// those in when no proxy variable is set. Inside Flatpak the portal decides per URL.
#[cfg(target_os = "linux")]
pub(crate) fn with_system_proxy(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    if env_http_proxy().is_some() {
        return builder;
    }

    if portal::in_flatpak() {
        return builder.proxy(portal::reqwest_proxy());
    }

    match platform_proxy() {
        Some(proxy) => proxy.apply(builder),
        None => builder,
    }
}

/// reqwest's `system-proxy` discovery covers the environment and the OS settings here.
#[cfg(not(target_os = "linux"))]
pub(crate) fn with_system_proxy(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder
}

#[cfg(target_os = "windows")]
fn platform_proxy() -> Option<SystemProxy> {
    windows::wininet_proxy().filter(|proxy| !proxy.is_empty())
}

#[cfg(target_os = "linux")]
fn platform_proxy() -> Option<SystemProxy> {
    let proxy = if portal::in_flatpak() {
        portal::system_proxy()
    } else {
        linux::desktop_proxy()
    };

    proxy.filter(|proxy| !proxy.is_empty())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn platform_proxy() -> Option<SystemProxy> {
    None
}

fn env_http_proxy() -> Option<String> {
    const KEYS: [&str; 6] = [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];

    for key in KEYS {
        let Ok(value) = std::env::var(key) else {
            continue;
        };

        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        return Some(with_proxy_scheme(value, "http://"));
    }

    None
}

fn with_proxy_scheme(addr: &str, fallback: &str) -> String {
    if addr.contains("://") {
        return addr.to_owned();
    }

    let mut url = String::with_capacity(fallback.len() + addr.len());
    url.push_str(fallback);
    url.push_str(addr);
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_net_has_no_proxy() {
        assert!(http_proxy_url(&NetConfig::direct()).is_none());
    }

    #[test]
    fn single_url_prefers_https_then_http_then_socks() {
        let proxy = SystemProxy {
            http: Some(String::from("http://h:1")),
            https: None,
            socks: Some(String::from("socks5://s:2")),
            bypass: Vec::new(),
        };

        assert_eq!(proxy.single_url().as_deref(), Some("http://h:1"));
    }
}
