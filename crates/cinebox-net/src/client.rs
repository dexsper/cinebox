//! Client cache and resilient send: system proxy first, direct DoH second.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tracing::warn;

use crate::NetConfig;
use crate::doh::DohResolve;

/// Cache key: one long-lived client per network config + per-service tuning.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ClientKey {
    net: NetConfig,
    connect_timeout: Duration,
    user_agent: Option<&'static str>,
    doh: bool,
}

static CLIENTS: OnceLock<Mutex<HashMap<ClientKey, reqwest::Client>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<ClientKey, reqwest::Client>> {
    CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_client(key: &ClientKey) -> Option<reqwest::Client> {
    let Ok(clients) = cache().lock() else {
        return None;
    };

    clients.get(key).cloned()
}

fn store_client(key: ClientKey, client: reqwest::Client) -> reqwest::Client {
    let Ok(mut clients) = cache().lock() else {
        return client;
    };

    clients.entry(key).or_insert(client).clone()
}

fn base_builder(
    connect_timeout: Duration,
    user_agent: Option<&'static str>,
) -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder().connect_timeout(connect_timeout);
    let Some(agent) = user_agent else {
        return builder;
    };

    builder.user_agent(agent)
}

/// Long-lived client without DoH: system proxy per `net.use_system_proxy`.
/// Set request timeouts with [`reqwest::RequestBuilder::timeout`].
///
/// # Errors
///
/// Returns the underlying `reqwest` error when the client cannot be built.
pub fn plain_client(
    net: &NetConfig,
    connect_timeout: Duration,
    user_agent: Option<&'static str>,
) -> Result<reqwest::Client, reqwest::Error> {
    let key = ClientKey {
        net: net.clone(),
        connect_timeout,
        user_agent,
        doh: false,
    };

    if let Some(client) = cached_client(&key) {
        return Ok(client);
    }

    let mut builder = base_builder(connect_timeout, user_agent);
    if !net.use_system_proxy {
        builder = builder.no_proxy();
    }

    let client = builder.build()?;
    Ok(store_client(key, client))
}

/// Direct client with the DoH resolver; never uses a proxy.
async fn doh_client(
    net: &NetConfig,
    connect_timeout: Duration,
    user_agent: Option<&'static str>,
) -> Result<reqwest::Client, reqwest::Error> {
    let key = ClientKey {
        net: net.clone(),
        connect_timeout,
        user_agent,
        doh: true,
    };

    if let Some(client) = cached_client(&key) {
        return Ok(client);
    }

    let Some(resolver) = DohResolve::new(&net.custom_doh_url).await else {
        warn!("doh resolver unavailable; not caching a system-dns client as doh");
        return base_builder(connect_timeout, user_agent).no_proxy().build();
    };

    let builder = base_builder(connect_timeout, user_agent).no_proxy();
    let client = builder.dns_resolver(Arc::new(resolver)).build()?;

    Ok(store_client(key, client))
}

/// Transport-level failures worth retrying over another path. HTTP statuses,
/// decode and redirect problems are legitimate server answers and are not.
fn is_transport_error(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout() || error.is_request()
}

/// Send a request, preferring the configured path but never depending on it.
///
/// With the system proxy on, the proxy goes first; if it fails at the
/// transport level and `dns_bypass` is enabled, the same request is rebuilt
/// and retried directly with DoH resolution. With the proxy off, DoH is used
/// outright when enabled. `build` may therefore run twice.
///
/// # Errors
///
/// Client build failures or the error of the last attempted path.
pub async fn send_resilient<F>(
    net: &NetConfig,
    connect_timeout: Duration,
    user_agent: Option<&'static str>,
    build: F,
) -> Result<reqwest::Response, reqwest::Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    if !net.use_system_proxy {
        if net.dns_bypass {
            let client = doh_client(net, connect_timeout, user_agent).await?;
            return build(&client).send().await;
        }

        let client = plain_client(net, connect_timeout, user_agent)?;
        return build(&client).send().await;
    }

    let primary = plain_client(net, connect_timeout, user_agent)?;

    send_primary_then_doh(net, connect_timeout, user_agent, &primary, build).await
}

async fn send_primary_then_doh<F>(
    net: &NetConfig,
    connect_timeout: Duration,
    user_agent: Option<&'static str>,
    primary: &reqwest::Client,
    build: F,
) -> Result<reqwest::Response, reqwest::Error>
where
    F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
{
    let error = match build(primary).send().await {
        Ok(response) => return Ok(response),
        Err(error) => error,
    };

    if !net.dns_bypass || !is_transport_error(&error) {
        return Err(error);
    }

    warn!(%error, "proxy request failed; retrying directly via DoH");
    let fallback = doh_client(net, connect_timeout, user_agent).await?;

    build(&fallback).send().await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(2);

    fn dead_proxy_client() -> reqwest::Client {
        let proxy = reqwest::Proxy::all("http://127.0.0.1:1").expect("proxy url");

        reqwest::Client::builder()
            .proxy(proxy)
            .connect_timeout(TIMEOUT)
            .build()
            .expect("client")
    }

    fn proxy_net(dns_bypass: bool) -> NetConfig {
        NetConfig {
            use_system_proxy: true,
            dns_bypass,
            custom_doh_url: String::new(),
        }
    }

    #[tokio::test]
    async fn transport_failure_retries_directly_when_bypass_is_on() {
        let calls = AtomicUsize::new(0);
        let primary = dead_proxy_client();
        let net = proxy_net(true);

        let result = send_primary_then_doh(&net, TIMEOUT, None, &primary, |client| {
            calls.fetch_add(1, Ordering::SeqCst);
            client.get("http://example.invalid/")
        })
        .await;

        assert!(result.is_err(), "dead proxy and fallback must both fail");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "expected a second attempt through the direct DoH client"
        );
    }

    #[tokio::test]
    async fn transport_failure_is_not_retried_without_bypass() {
        let calls = AtomicUsize::new(0);
        let primary = dead_proxy_client();
        let net = proxy_net(false);

        let result = send_primary_then_doh(&net, TIMEOUT, None, &primary, |client| {
            calls.fetch_add(1, Ordering::SeqCst);
            client.get("http://example.invalid/")
        })
        .await;

        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no retry with bypass off");
    }
}
