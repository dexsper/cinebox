//! InnerTube `player` resolve: watch page → player JS → signed URLs.

use cinebox_net::NetConfig;
use serde::Serialize;

use crate::cipher::{self, Decipher};
use crate::error::Error;
use crate::formats::{pick, CipherParts, PlayerResponse};
use crate::http::{
    send_json, send_text, ANDROID_CLIENT, ANDROID_CLIENT_NAME, ANDROID_UA, ANDROID_VERSION,
    REQUEST_TIMEOUT,
};
use crate::id::VideoId;

/// Signed media URLs plus the Android UA for libmpv.
#[derive(Debug, Clone, PartialEq)]
pub struct Playback {
    pub video_url: String,
    pub audio_url: Option<String>,
    pub http_header_fields: Vec<String>,
    pub title: Option<String>,
    pub duration: Option<f64>,
}

/// Fetch InnerTube player JSON, decipher signatures, pick muxed or A/V URLs.
///
/// `origin` is `https://www.youtube.com` in production and an httpmock base URL in tests.
/// Watch HTML and player JS are dropped after sts/sig/nsig are extracted.
///
/// # Errors
///
/// HTTP, playability, missing player JS, cipher, or format failures.
pub async fn resolve(origin: &str, id: &VideoId, net: &NetConfig) -> Result<Playback, Error> {
    tracing::debug!(id = id.as_str(), "youtube resolve");

    let html = fetch_watch(origin, id, net).await?;
    let player_path = cipher::player_js_url(&html).ok_or(Error::NoPlayer)?;
    let player_url = join_origin(origin, player_path)?;
    drop(html);

    let js = fetch_js(net, &player_url).await?;
    let sts = cipher::signature_timestamp(&js);
    let player = fetch_player(origin, id, sts, net).await?;

    player.ensure_playable()?;

    let meta = player.meta();
    let mut pending = Vec::new();

    for fmt in player.raw_formats() {
        if fmt.skip() {
            continue;
        }

        let parts = fmt.cipher_or_url()?;
        pending.push((parts, fmt.clone()));
    }

    if pending.is_empty() {
        return Err(Error::NoFormats);
    }

    let need_js = pending.iter().any(|(parts, _)| needs_js(parts));
    let mut decipher = None;

    if need_js {
        decipher = Some(Decipher::new(js));
    } else {
        drop(js);
    }

    let mut streams = Vec::with_capacity(pending.len());
    let mut duration = meta.duration;

    for (parts, fmt) in pending {
        let url = decode_url(decipher.as_mut(), parts)?;
        let Some(stream) = fmt.to_stream(url) else {
            continue;
        };

        if duration.is_none() {
            duration = fmt.duration_secs();
        }

        streams.push(stream);
    }

    drop(decipher);

    let (video_url, audio_url) = pick(&streams)?;
    let http_header_fields = ua_header();

    Ok(Playback {
        video_url,
        audio_url,
        http_header_fields,
        title: meta.title,
        duration,
    })
}

fn needs_js(parts: &CipherParts) -> bool {
    if parts.sig.is_some() {
        return true;
    }

    query_param(&parts.url, "n").is_some()
}

fn decode_url(mut decipher: Option<&mut Decipher>, parts: CipherParts) -> Result<String, Error> {
    let mut url = parts.url;

    if let Some(s) = parts.sig.as_deref() {
        let Some(d) = decipher.as_mut() else {
            return Err(Error::BadSig);
        };

        let sig = d.decrypt_sig(s)?;
        let key = parts.sp.as_deref().unwrap_or("signature");
        url = set_query(&url, key, &sig)?;
    }

    let Some(n) = query_param(&url, "n") else {
        return Ok(url);
    };

    let Some(d) = decipher else {
        return Err(Error::BadNsig);
    };

    let plain = d.decrypt_n(&n)?;

    set_query(&url, "n", &plain)
}

async fn fetch_watch(origin: &str, id: &VideoId, net: &NetConfig) -> Result<String, Error> {
    tracing::debug!(id = id.as_str(), "youtube watch page");

    let url = watch_url(origin, id.as_str());

    send_text(net, |client| client.get(&url).timeout(REQUEST_TIMEOUT)).await
}

async fn fetch_js(net: &NetConfig, url: &str) -> Result<String, Error> {
    tracing::debug!("youtube player js");

    send_text(net, |client| client.get(url).timeout(REQUEST_TIMEOUT)).await
}

async fn fetch_player(
    origin: &str,
    id: &VideoId,
    sts: Option<u32>,
    net: &NetConfig,
) -> Result<PlayerResponse, Error> {
    tracing::debug!(id = id.as_str(), ?sts, "youtube innertube");

    let api_url = player_api_url(origin);
    let body = player_body(id.as_str(), sts);

    send_json(net, |client| {
        client
            .post(&api_url)
            .timeout(REQUEST_TIMEOUT)
            .header("Content-Type", "application/json")
            .header("Origin", origin)
            .header("X-YouTube-Client-Name", ANDROID_CLIENT_NAME)
            .header("X-YouTube-Client-Version", ANDROID_VERSION)
            .json(&body)
    })
    .await
}

fn watch_url(origin: &str, id: &str) -> String {
    let origin = origin.trim_end_matches('/');
    let mut url = String::with_capacity(origin.len() + 32 + id.len());
    url.push_str(origin);
    url.push_str("/watch?v=");
    url.push_str(id);
    url.push_str("&bpctr=9999999999&has_verified=1");
    url
}

fn player_api_url(origin: &str) -> String {
    let origin = origin.trim_end_matches('/');
    let mut url = String::with_capacity(origin.len() + 40);
    url.push_str(origin);
    url.push_str("/youtubei/v1/player?prettyPrint=false");
    url
}

fn join_origin(origin: &str, found: &str) -> Result<String, Error> {
    if found.starts_with("https://") || found.starts_with("http://") {
        return Ok(found.to_owned());
    }

    if let Some(rest) = found.strip_prefix("//") {
        let Some(scheme_end) = origin.find("://") else {
            return Err(Error::NoPlayer);
        };

        let scheme = &origin[..scheme_end];
        let mut url = String::with_capacity(scheme.len() + 3 + rest.len());
        url.push_str(scheme);
        url.push_str("://");
        url.push_str(rest);

        return Ok(url);
    }

    let origin = origin.trim_end_matches('/');
    let mut url = String::with_capacity(origin.len() + found.len() + 1);
    url.push_str(origin);

    if found.starts_with('/') {
        url.push_str(found);
        return Ok(url);
    }

    url.push('/');
    url.push_str(found);

    Ok(url)
}

fn ua_header() -> Vec<String> {
    let mut line = String::with_capacity(12 + ANDROID_UA.len());
    line.push_str("User-Agent: ");
    line.push_str(ANDROID_UA);

    vec![line]
}

#[derive(Serialize)]
struct PlayerBody<'a> {
    context: InnertubeContext<'a>,
    #[serde(rename = "videoId")]
    video_id: &'a str,
    #[serde(rename = "playbackContext")]
    playback_context: PlaybackContext,
    #[serde(rename = "contentCheckOk")]
    content_check_ok: bool,
    #[serde(rename = "racyCheckOk")]
    racy_check_ok: bool,
}

#[derive(Serialize)]
struct InnertubeContext<'a> {
    client: InnertubeClient<'a>,
}

#[derive(Serialize)]
struct InnertubeClient<'a> {
    #[serde(rename = "clientName")]
    client_name: &'a str,
    #[serde(rename = "clientVersion")]
    client_version: &'a str,
    #[serde(rename = "userAgent")]
    user_agent: &'a str,
    #[serde(rename = "osName")]
    os_name: &'a str,
    #[serde(rename = "osVersion")]
    os_version: &'a str,
    hl: &'a str,
    #[serde(rename = "timeZone")]
    time_zone: &'a str,
    #[serde(rename = "utcOffsetMinutes")]
    utc_offset_minutes: i32,
}

#[derive(Serialize)]
struct PlaybackContext {
    #[serde(rename = "contentPlaybackContext")]
    content: ContentPlayback,
}

#[derive(Serialize)]
struct ContentPlayback {
    #[serde(rename = "html5Preference")]
    html5_preference: &'static str,
    #[serde(rename = "signatureTimestamp", skip_serializing_if = "Option::is_none")]
    signature_timestamp: Option<u32>,
}

fn player_body<'a>(video_id: &'a str, sts: Option<u32>) -> PlayerBody<'a> {
    PlayerBody {
        context: InnertubeContext {
            client: InnertubeClient {
                client_name: ANDROID_CLIENT,
                client_version: ANDROID_VERSION,
                user_agent: ANDROID_UA,
                os_name: "Android",
                os_version: "11",
                hl: "en",
                time_zone: "UTC",
                utc_offset_minutes: 0,
            },
        },
        video_id,
        playback_context: PlaybackContext {
            content: ContentPlayback {
                html5_preference: "HTML5_PREF_WANTS",
                signature_timestamp: sts,
            },
        },
        content_check_ok: true,
        racy_check_ok: true,
    }
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return None;
    };

    for (k, v) in parsed.query_pairs() {
        if k == key {
            return Some(v.into_owned());
        }
    }

    None
}

fn set_query(url: &str, key: &str, value: &str) -> Result<String, Error> {
    let mut parsed = reqwest::Url::parse(url).map_err(|_| Error::BadCipher)?;
    let mut updated = Vec::new();
    let mut found = false;

    for (k, v) in parsed.query_pairs() {
        if k == key {
            updated.push((k.into_owned(), value.to_owned()));
            found = true;
            continue;
        }

        updated.push((k.into_owned(), v.into_owned()));
    }

    if !found {
        updated.push((key.to_owned(), value.to_owned()));
    }

    parsed.query_pairs_mut().clear().extend_pairs(&updated);

    Ok(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use cinebox_net::NetConfig;
    use httpmock::prelude::*;

    use super::*;

    const VID: &str = "abcdefghijk";
    const PLAYER_PATH: &str = "/s/player/deadbeef/player_ias.vflset/en_US/base.js";

    const PLAYER_JS: &str = r#"
var sig=function(a){a=a.split("");a.reverse();return a.join("")};
var nnn=function(a){a=a.split("");a.reverse();var b=a.join("");return b;return "zz_w8_"+a};
signatureTimestamp:12345
"#;

    fn net() -> NetConfig {
        NetConfig::direct()
    }

    fn id() -> VideoId {
        VideoId::parse(VID).unwrap_or_else(|_| panic!("id"))
    }

    fn watch_html() -> String {
        let mut html = String::from(r#"<html><script>ytcfg.set({"PLAYER_JS_URL":""#);
        html.push_str(PLAYER_PATH);
        html.push_str(r#""});</script></html>"#);
        html
    }

    fn muxed_json(url: &str) -> serde_json::Value {
        serde_json::json!({
            "playabilityStatus": { "status": "OK" },
            "videoDetails": { "title": "Trailer", "lengthSeconds": "120" },
            "streamingData": {
                "formats": [{
                    "itag": 18,
                    "url": url,
                    "mimeType": "video/mp4; codecs=\"avc1.42001E, mp4a.40.2\"",
                    "bitrate": 500000,
                    "width": 640,
                    "height": 360
                }]
            }
        })
    }

    async fn mock_watch_and_js(server: &MockServer) {
        server
            .mock_async(|when, then| {
                when.method(GET).path("/watch");
                then.status(200).body(watch_html());
            })
            .await;

        server
            .mock_async(|when, then| {
                when.method(GET).path(PLAYER_PATH);
                then.status(200).body(PLAYER_JS);
            })
            .await;
    }

    #[tokio::test]
    async fn test_plain_url() {
        let server = MockServer::start_async().await;
        mock_watch_and_js(&server).await;

        let media = "https://googlevideo.example/v?rate=1";
        server
            .mock_async(|when, then| {
                when.method(POST).path("/youtubei/v1/player");
                then.status(200).json_body(muxed_json(media));
            })
            .await;

        let play = resolve(&server.base_url(), &id(), &net())
            .await
            .unwrap_or_else(|e| panic!("{e}"));

        assert_eq!(play.video_url, media);
        assert!(play.audio_url.is_none());
        assert_eq!(play.title.as_deref(), Some("Trailer"));
        assert_eq!(play.duration, Some(120.0));
        assert!(play.http_header_fields.iter().any(|h| h.contains(ANDROID_UA)));
    }

    #[tokio::test]
    async fn test_signature_cipher() {
        let server = MockServer::start_async().await;
        mock_watch_and_js(&server).await;

        let cipher = "url=https%3A%2F%2Fgooglevideo.example%2Fv&s=dcba&sp=sig";
        server
            .mock_async(|when, then| {
                when.method(POST).path("/youtubei/v1/player");
                then.status(200).json_body(serde_json::json!({
                    "playabilityStatus": { "status": "OK" },
                    "streamingData": {
                        "formats": [{
                            "itag": 18,
                            "signatureCipher": cipher,
                            "mimeType": "video/mp4; codecs=\"avc1.42001E, mp4a.40.2\"",
                            "bitrate": 500000,
                            "height": 360
                        }]
                    }
                }));
            })
            .await;

        let play = resolve(&server.base_url(), &id(), &net())
            .await
            .unwrap_or_else(|e| panic!("{e}"));

        let sig = query_param(&play.video_url, "sig");
        assert_eq!(sig.as_deref(), Some("abcd"));
    }

    #[tokio::test]
    async fn test_nsig() {
        let server = MockServer::start_async().await;
        mock_watch_and_js(&server).await;

        let media = "https://googlevideo.example/v?n=xyz&rate=1";
        server
            .mock_async(|when, then| {
                when.method(POST).path("/youtubei/v1/player");
                then.status(200).json_body(muxed_json(media));
            })
            .await;

        let play = resolve(&server.base_url(), &id(), &net())
            .await
            .unwrap_or_else(|e| panic!("{e}"));

        let n = query_param(&play.video_url, "n");
        assert_eq!(n.as_deref(), Some("zyx"));
    }

    #[tokio::test]
    async fn test_playability_error() {
        let server = MockServer::start_async().await;
        mock_watch_and_js(&server).await;

        server
            .mock_async(|when, then| {
                when.method(POST).path("/youtubei/v1/player");
                then.status(200).json_body(serde_json::json!({
                    "playabilityStatus": { "status": "UNPLAYABLE" }
                }));
            })
            .await;

        let result = resolve(&server.base_url(), &id(), &net()).await;
        assert!(matches!(result, Err(Error::Unplayable)), "{result:?}");
    }

    #[tokio::test]
    async fn test_adaptive_pair() {
        let server = MockServer::start_async().await;
        mock_watch_and_js(&server).await;

        server
            .mock_async(|when, then| {
                when.method(POST).path("/youtubei/v1/player");
                then.status(200).json_body(serde_json::json!({
                    "playabilityStatus": { "status": "OK" },
                    "streamingData": {
                        "adaptiveFormats": [
                            {
                                "itag": 137,
                                "url": "https://googlevideo.example/video",
                                "mimeType": "video/mp4; codecs=\"avc1.640028\"",
                                "bitrate": 2000000,
                                "width": 1920,
                                "height": 1080
                            },
                            {
                                "itag": 140,
                                "url": "https://googlevideo.example/audio",
                                "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"",
                                "bitrate": 128000,
                                "audioQuality": "AUDIO_QUALITY_MEDIUM"
                            }
                        ]
                    }
                }));
            })
            .await;

        let play = resolve(&server.base_url(), &id(), &net())
            .await
            .unwrap_or_else(|e| panic!("{e}"));

        assert_eq!(play.video_url, "https://googlevideo.example/video");
        assert_eq!(play.audio_url.as_deref(), Some("https://googlevideo.example/audio"));
    }

    #[tokio::test]
    #[ignore = "hits live youtube"]
    async fn resolve_reaches_youtube() {
        let net = NetConfig {
            use_system_proxy: true,
            dns_bypass: true,
            custom_doh_url: String::new(),
        };
        let id = VideoId::parse("jNQXAC9IVRw").unwrap_or_else(|_| panic!("id"));
        let result = resolve("https://www.youtube.com", &id, &net).await;

        assert!(
            result.is_ok() || matches!(result, Err(Error::Unplayable | Error::Http(_) | Error::Request(_))),
            "{result:?}"
        );
    }
}
