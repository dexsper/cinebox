//! WinINet manual proxy from the registry (Settings > Network > Proxy).

use super::{SystemProxy, with_proxy_scheme};

pub(super) fn wininet_proxy() -> Option<SystemProxy> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;

    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    let server: String = key.get_value("ProxyServer").unwrap_or_default();
    let pac: String = key.get_value("AutoConfigURL").unwrap_or_default();

    if enabled == 0 {
        if !pac.trim().is_empty() {
            tracing::warn!("system proxy is a pac script; it is not evaluated");
        }

        return None;
    }

    let server = server.trim();
    if server.is_empty() {
        return None;
    }

    Some(parse_proxy_server(server))
}

/// `ProxyServer` is either one `host:port` for every protocol or a
/// `http=...;https=...;socks=...` list.
fn parse_proxy_server(raw: &str) -> SystemProxy {
    if !raw.contains('=') {
        let url = with_proxy_scheme(raw.trim(), "http://");

        return SystemProxy {
            http: Some(url.clone()),
            https: Some(url),
            ..SystemProxy::default()
        };
    }

    let mut proxy = SystemProxy::default();
    for part in raw.split(';') {
        let Some((kind, addr)) = part.split_once('=') else {
            continue;
        };

        let addr = addr.trim();
        if addr.is_empty() {
            continue;
        }

        let kind = kind.trim();
        if kind.eq_ignore_ascii_case("http") {
            proxy.http = Some(with_proxy_scheme(addr, "http://"));
        } else if kind.eq_ignore_ascii_case("https") {
            proxy.https = Some(with_proxy_scheme(addr, "http://"));
        } else if kind.eq_ignore_ascii_case("socks") {
            proxy.socks = Some(with_proxy_scheme(addr, "socks5://"));
        }
    }

    proxy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream_url(raw: &str) -> Option<String> {
        parse_proxy_server(raw).single_url()
    }

    #[test]
    fn bare_host_port_gets_http_scheme() {
        assert_eq!(
            stream_url("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn keeps_existing_scheme() {
        assert_eq!(
            stream_url("http://127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn protocol_list_prefers_https_then_http() {
        let raw = "http=127.0.0.1:7890;https=127.0.0.1:7890;socks=127.0.0.1:7891";
        assert_eq!(stream_url(raw).as_deref(), Some("http://127.0.0.1:7890"));
    }

    #[test]
    fn socks_only_uses_socks5() {
        assert_eq!(
            stream_url("socks=127.0.0.1:1080").as_deref(),
            Some("socks5://127.0.0.1:1080")
        );
    }
}
