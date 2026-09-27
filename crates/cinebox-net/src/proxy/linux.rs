//! Desktop proxy settings: KDE (`kioslaverc`) and GNOME-family desktops (GSettings).

use std::path::PathBuf;
use std::process::Command;

use super::{SystemProxy, with_proxy_scheme};

pub(super) fn desktop_proxy() -> Option<SystemProxy> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let kde = desktop
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("KDE"));

    if kde {
        let text = std::fs::read_to_string(kioslaverc_path()?).ok()?;
        return parse_kioslaverc(&text);
    }

    gnome_proxy(gsettings_get)
}

fn kioslaverc_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(config.join("kioslaverc"))
}

fn gsettings_get(schema: &str, key: &str) -> Option<String> {
    let output = Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
}

/// `get(schema, key)` returns a value in GVariant text form, e.g. `'manual'` or `3128`.
fn gnome_proxy(get: impl Fn(&str, &str) -> Option<String>) -> Option<SystemProxy> {
    let mode = get("org.gnome.system.proxy", "mode")?;
    match unquote(&mode) {
        "manual" => {}
        "auto" => {
            tracing::warn!("desktop proxy is a pac script; it is not evaluated");
            return None;
        }
        _ => return None,
    }

    let endpoint = |kind: &str, scheme: &str| {
        let schema = format!("org.gnome.system.proxy.{kind}");
        let host = get(&schema, "host")?;
        let host = unquote(&host);
        let port: u16 = get(&schema, "port")?.parse().ok()?;
        if host.is_empty() || port == 0 {
            return None;
        }

        Some(with_proxy_scheme(&format!("{host}:{port}"), scheme))
    };

    let bypass = get("org.gnome.system.proxy", "ignore-hosts")
        .map(|list| parse_gvariant_strings(&list))
        .unwrap_or_default();

    Some(SystemProxy {
        http: endpoint("http", "http://"),
        https: endpoint("https", "http://"),
        socks: endpoint("socks", "socks5://"),
        bypass: bypass.iter().map(|host| bypass_rule(host)).collect(),
    })
}

/// `['localhost', '127.0.0.0/8']`, or `@as []` when empty.
fn parse_gvariant_strings(raw: &str) -> Vec<String> {
    let inner = raw
        .trim()
        .trim_start_matches("@as")
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');

    inner
        .split(',')
        .map(|item| unquote(item.trim()).to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

fn unquote(value: &str) -> &str {
    value.trim().trim_matches('\'').trim_matches('"')
}

/// KDE `ProxyType`: 0 none, 1 manual, 2 PAC URL, 3 WPAD, 4 environment variables.
fn parse_kioslaverc(text: &str) -> Option<SystemProxy> {
    let mut in_section = false;
    let mut kind = None;
    let mut proxy = SystemProxy::default();
    let mut reversed = false;

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[Proxy Settings]";
            continue;
        }

        if !in_section {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        // Keys may carry KConfig flags such as `NoProxyFor[$e]`.
        let key = key.split('[').next().unwrap_or(key).trim();
        let value = value.trim();
        match key {
            "ProxyType" => kind = value.parse::<u8>().ok(),
            "httpProxy" => proxy.http = kde_endpoint(value, "http://"),
            "httpsProxy" => proxy.https = kde_endpoint(value, "http://"),
            "socksProxy" => proxy.socks = kde_endpoint(value, "socks5://"),
            "NoProxyFor" => {
                proxy.bypass = value
                    .split(',')
                    .map(str::trim)
                    .filter(|host| !host.is_empty())
                    .map(bypass_rule)
                    .collect();
            }
            "ReversedException" => reversed = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    match kind? {
        1 if reversed => {
            tracing::warn!("desktop proxy applies only to listed hosts; that mode is not supported");
            None
        }
        1 => Some(proxy),
        2 | 3 => {
            tracing::warn!("desktop proxy is a pac script or wpad; it is not evaluated");
            None
        }
        _ => None,
    }
}

/// Values look like `http://host:port`, `http://host port`, or `host port`;
/// SOCKS uses the `socks://` scheme.
fn kde_endpoint(value: &str, scheme: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let address = match value.rsplit_once(' ') {
        Some((host, port)) => format!("{}:{}", host.trim(), port.trim()),
        None => value.to_owned(),
    };
    let address = address.replacen("socks://", "socks5://", 1);

    Some(with_proxy_scheme(&address, scheme))
}

/// Desktop wildcards like `*.example.com` become reqwest's `.example.com` suffix rule.
fn bypass_rule(host: &str) -> String {
    host.strip_prefix('*').unwrap_or(host).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod gnome_proxy {
        use super::*;

        fn settings(mode: &str) -> impl Fn(&str, &str) -> Option<String> {
            let mode = format!("'{mode}'");
            move |schema, key| {
                let value = match (schema, key) {
                    ("org.gnome.system.proxy", "mode") => mode.as_str(),
                    ("org.gnome.system.proxy", "ignore-hosts") => "['localhost', '*.lan', '10.0.0.0/8']",
                    ("org.gnome.system.proxy.http", "host") => "'proxy.lan'",
                    ("org.gnome.system.proxy.http", "port") => "3128",
                    ("org.gnome.system.proxy.https", "host") => "''",
                    ("org.gnome.system.proxy.https", "port") => "0",
                    ("org.gnome.system.proxy.socks", "host") => "'127.0.0.1'",
                    ("org.gnome.system.proxy.socks", "port") => "1080",
                    _ => return None,
                };
                Some(value.to_owned())
            }
        }

        #[test]
        fn manual_mode_reads_every_endpoint_and_bypass_list() {
            let proxy = gnome_proxy(settings("manual"));

            assert_eq!(
                proxy,
                Some(SystemProxy {
                    http: Some(String::from("http://proxy.lan:3128")),
                    https: None,
                    socks: Some(String::from("socks5://127.0.0.1:1080")),
                    bypass: vec![
                        String::from("localhost"),
                        String::from(".lan"),
                        String::from("10.0.0.0/8"),
                    ],
                })
            );
        }

        #[test]
        fn none_mode_has_no_proxy() {
            assert_eq!(gnome_proxy(settings("none")), None);
        }

        #[test]
        fn auto_mode_is_not_evaluated() {
            assert_eq!(gnome_proxy(settings("auto")), None);
        }

        #[test]
        fn missing_gsettings_has_no_proxy() {
            assert_eq!(gnome_proxy(|_, _| None), None);
        }
    }

    mod parse_kioslaverc {
        use super::*;

        #[test]
        fn manual_proxy_accepts_space_separated_ports() {
            let text = "[Proxy Settings]\n\
                        ProxyType=1\n\
                        httpProxy=http://proxy.lan 3128\n\
                        httpsProxy=http://proxy.lan:3129\n\
                        socksProxy=socks://127.0.0.1 1080\n\
                        NoProxyFor[$e]=localhost,*.lan\n";

            assert_eq!(
                parse_kioslaverc(text),
                Some(SystemProxy {
                    http: Some(String::from("http://proxy.lan:3128")),
                    https: Some(String::from("http://proxy.lan:3129")),
                    socks: Some(String::from("socks5://127.0.0.1:1080")),
                    bypass: vec![String::from("localhost"), String::from(".lan")],
                })
            );
        }

        #[test]
        fn keys_outside_the_proxy_section_are_ignored() {
            let text = "[General]\nProxyType=1\n[Proxy Settings]\nProxyType=0\n";

            assert_eq!(parse_kioslaverc(text), None);
        }

        #[test]
        fn reversed_exceptions_are_not_applied() {
            let text = "[Proxy Settings]\nProxyType=1\nhttpProxy=http://p 8080\nReversedException=true\n";

            assert_eq!(parse_kioslaverc(text), None);
        }
    }
}
