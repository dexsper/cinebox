//! HTTP-level tests against a mock TorrServer (httpmock) instead of a live one.

use cinebox_torrserver::{AddSpec, Error, add, echo, get, list, viewed_list};
use httpmock::prelude::*;

#[tokio::test]
async fn echo_returns_version() -> Result<(), Error> {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/echo");
            then.status(200).body("MatriX.134");
        })
        .await;

    let version = echo(&server.base_url(), "", "").await?;
    assert_eq!(version, "MatriX.134");

    Ok(())
}

#[tokio::test]
async fn echo_maps_unauthorized_to_http_401() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/echo");
            then.status(401);
        })
        .await;

    let result = echo(&server.base_url(), "", "").await;
    assert!(matches!(result, Err(Error::Http(401))), "{result:?}");
}

#[tokio::test]
async fn get_maps_404_to_not_found() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/torrents");
            then.status(404);
        })
        .await;

    let result = get(&server.base_url(), "", "", "abc").await;
    assert!(matches!(result, Err(Error::NotFound)), "{result:?}");
}

#[tokio::test]
async fn list_parses_rows_and_skips_hashless() -> Result<(), Error> {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/torrents")
                .json_body_includes(r#"{ "action": "list" }"#);
            then.status(200).json_body_obj(&serde_json::json!([
                { "hash": "aa", "title": "Kept" },
                { "title": "No hash" },
                { "hash": "", "title": "Empty hash" }
            ]));
        })
        .await;

    let rows = list(&server.base_url(), "", "").await?;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].hash, "aa");
    assert_eq!(rows[0].title, "Kept");

    Ok(())
}

#[tokio::test]
async fn add_sends_basic_auth_and_parses_status() -> Result<(), Error> {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/torrents")
                .header("authorization", "Basic dXNlcjpwYXNz")
                .json_body_includes(r#"{ "action": "add", "link": "magnet:?xt=urn:btih:aa" }"#);

            then.status(200).json_body_obj(&serde_json::json!({
                "hash": "aa",
                "stat": 2,
                "file_stats": [{ "id": 1, "path": "a.mkv", "length": 10 }]
            }));
        })
        .await;

    let spec = AddSpec {
        link: String::from("magnet:?xt=urn:btih:aa"),
        title: String::from("T"),
        poster: String::new(),
        category: String::new(),
        save_to_db: false,
    };

    let status = add(&server.base_url(), "user", "pass", &spec).await?;

    mock.assert_async().await;
    assert_eq!(status.hash, "aa");
    assert_eq!(status.file_stats.len(), 1);

    Ok(())
}

#[tokio::test]
async fn viewed_list_maps_bad_json_to_error() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/viewed");
            then.status(200).body("not json");
        })
        .await;

    let result = viewed_list(&server.base_url(), "", "", "abc").await;
    assert!(matches!(result, Err(Error::BadJson(_))), "{result:?}");
}
