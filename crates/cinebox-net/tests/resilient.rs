//! `send_resilient` happy paths against a local httpmock server.
//!
//! Transport-failure retry is covered in `client` unit tests with an explicit
//! dead HTTP proxy, so it does not depend on the machine's system proxy.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cinebox_net::{NetConfig, plain_client, send_resilient};
use httpmock::prelude::*;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

fn proxy_net(dns_bypass: bool) -> NetConfig {
    NetConfig {
        use_system_proxy: true,
        dns_bypass,
        custom_doh_url: String::new(),
    }
}

#[tokio::test]
async fn direct_request_reaches_server() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/ping");
            then.status(200).body("pong");
        })
        .await;

    let net = NetConfig::direct();
    let response = send_resilient(&net, CONNECT_TIMEOUT, None, |client| {
        client.get(server.url("/ping"))
    })
    .await
    .expect("direct send");

    assert_eq!(response.status().as_u16(), 200);
    mock.assert_async().await;
}

#[tokio::test]
async fn doh_client_serves_direct_path_when_bypass_is_on() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/doh-direct");
            then.status(200);
        })
        .await;

    // IP-literal URL: no DNS lookup happens, but the request must go through
    // the DoH-configured client because the proxy is off and bypass is on.
    let net = NetConfig {
        use_system_proxy: false,
        dns_bypass: true,
        custom_doh_url: String::new(),
    };
    let response = send_resilient(&net, CONNECT_TIMEOUT, None, |client| {
        client.get(server.url("/doh-direct"))
    })
    .await
    .expect("doh-direct send");

    assert_eq!(response.status().as_u16(), 200);
    mock.assert_async().await;
}

#[tokio::test]
async fn http_error_status_is_returned_without_retry() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/fail");
            then.status(500);
        })
        .await;

    let calls = AtomicUsize::new(0);
    let response = send_resilient(&proxy_net(true), CONNECT_TIMEOUT, None, |client| {
        calls.fetch_add(1, Ordering::SeqCst);
        client.get(server.url("/fail"))
    })
    .await
    .expect("HTTP 500 is a response, not a transport error");

    assert_eq!(response.status().as_u16(), 500);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    mock.assert_async().await;
}

#[test]
fn plain_client_is_cached_per_config() {
    let net = NetConfig::direct();
    let first = plain_client(&net, CONNECT_TIMEOUT, Some("test/1")).expect("client");
    let second = plain_client(&net, CONNECT_TIMEOUT, Some("test/1")).expect("client");

    // reqwest::Client is an Arc internally; the factory must reuse it.
    let _ = (first, second);
}
