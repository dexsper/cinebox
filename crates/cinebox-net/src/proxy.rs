//! Discover the process HTTP proxy for clients that are not reqwest (libmpv).

use crate::NetConfig;

/// Proxy URL for stream downloads when [`NetConfig::use_system_proxy`] is on.
///
/// Reads `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY`, then the Windows WinINet
/// manual proxy. PAC scripts are not evaluated. Do not log the return value:
/// it may contain credentials.
#[must_use]
pub fn http_proxy_url(net: &NetConfig) -> Option<String> {
    if !net.use_system_proxy {
        return None;
    }

    if let Some(url) = env_http_proxy() {
        return Some(url);
    }

    #[cfg(windows)]
    if let Some(url) = wininet_proxy() {
        return Some(url);
    }

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

#[cfg(windows)]
fn wininet_proxy() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;

    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    let server: String = key.get_value("ProxyServer").unwrap_or_default();
    let pac: String = key.get_value("AutoConfigURL").unwrap_or_default();

    if enabled == 0 {
        if !pac.trim().is_empty() {
            tracing::warn!("system proxy is a pac script; mpv cannot use it");
        }

        return None;
    }

    let server = server.trim();
    if server.is_empty() {
        return None;
    }

    parse_wininet_proxy_server(server)
}

fn parse_wininet_proxy_server(raw: &str) -> Option<String> {
    if !raw.contains('=') {
        return Some(with_proxy_scheme(raw.trim(), "http://"));
    }

    let mut http = None;
    let mut https = None;
    let mut socks = None;

    for part in raw.split(';') {
        let Some((kind, addr)) = part.split_once('=') else {
            continue;
        };

        let addr = addr.trim();
        if addr.is_empty() {
            continue;
        }

        let kind = kind.trim();
        if kind.eq_ignore_ascii_case("https") {
            https = Some(addr);
            continue;
        }

        if kind.eq_ignore_ascii_case("http") {
            http = Some(addr);
            continue;
        }

        if kind.eq_ignore_ascii_case("socks") {
            socks = Some(addr);
        }
    }

    if let Some(addr) = https.or(http) {
        return Some(with_proxy_scheme(addr, "http://"));
    }

    socks.map(|addr| with_proxy_scheme(addr, "socks5://"))
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
    fn bare_host_port_gets_http_scheme() {
        assert_eq!(
            parse_wininet_proxy_server("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn keeps_existing_scheme() {
        assert_eq!(
            parse_wininet_proxy_server("http://127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn protocol_list_prefers_https_then_http() {
        let raw = "http=127.0.0.1:7890;https=127.0.0.1:7890;socks=127.0.0.1:7891";
        assert_eq!(
            parse_wininet_proxy_server(raw).as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn socks_only_uses_socks5() {
        assert_eq!(
            parse_wininet_proxy_server("socks=127.0.0.1:1080").as_deref(),
            Some("socks5://127.0.0.1:1080")
        );
    }

    #[test]
    fn disabled_net_has_no_proxy() {
        assert!(http_proxy_url(&NetConfig::direct()).is_none());
    }
}
