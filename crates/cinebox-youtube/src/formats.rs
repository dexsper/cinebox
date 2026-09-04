//! InnerTube player JSON and format selection.

use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Deserialize)]
pub(crate) struct PlayerResponse {
    #[serde(default, rename = "playabilityStatus")]
    playability_status: Option<Playability>,
    #[serde(default, rename = "streamingData")]
    streaming_data: Option<StreamingData>,
    #[serde(default, rename = "videoDetails")]
    video_details: Option<VideoDetails>,
}

#[derive(Debug, Deserialize)]
struct Playability {
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamingData {
    #[serde(default)]
    formats: Option<Vec<RawFormat>>,
    #[serde(default, rename = "adaptiveFormats")]
    adaptive_formats: Option<Vec<RawFormat>>,
}

#[derive(Debug, Deserialize)]
struct VideoDetails {
    #[serde(default)]
    title: Option<String>,
    #[serde(default, rename = "lengthSeconds")]
    length_seconds: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawFormat {
    #[serde(default)]
    url: Option<String>,
    #[serde(default, rename = "signatureCipher")]
    signature_cipher: Option<String>,
    #[serde(default)]
    cipher: Option<String>,
    #[serde(default, rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(default)]
    bitrate: Option<u64>,
    #[serde(default, rename = "averageBitrate")]
    average_bitrate: Option<u64>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default, rename = "targetDurationSec")]
    target_duration_sec: Option<u32>,
    #[serde(default, rename = "type")]
    format_type: Option<String>,
    #[serde(default, rename = "audioQuality")]
    audio_quality: Option<String>,
    #[serde(default, rename = "drmFamilies")]
    drm_families: Option<serde_json::Value>,
    #[serde(default, rename = "approxDurationMs")]
    approx_duration_ms: Option<serde_json::Value>,
}

pub(crate) struct Stream {
    pub url: String,
    pub height: u32,
    pub bitrate: u64,
    pub has_video: bool,
    pub has_audio: bool,
}

pub(crate) struct PlayerMeta {
    pub title: Option<String>,
    pub duration: Option<f64>,
}

impl PlayerResponse {
    pub(crate) fn ensure_playable(&self) -> Result<(), Error> {
        let Some(status) = self.playability_status.as_ref().and_then(|p| p.status.as_deref()) else {
            return Ok(());
        };

        if status == "OK" {
            return Ok(());
        }

        Err(Error::Unplayable)
    }

    pub(crate) fn meta(&self) -> PlayerMeta {
        let details = self.video_details.as_ref();
        let title = details.and_then(|d| d.title.clone());
        let duration = details.and_then(|d| json_f64(&d.length_seconds));

        PlayerMeta { title, duration }
    }

    pub(crate) fn raw_formats(&self) -> Vec<&RawFormat> {
        let mut out = Vec::new();

        if let Some(data) = &self.streaming_data {
            if let Some(formats) = &data.formats {
                out.extend(formats.iter());
            }

            if let Some(formats) = &data.adaptive_formats {
                out.extend(formats.iter());
            }
        }

        out
    }
}

impl RawFormat {
    pub(crate) fn skip(&self) -> bool {
        if self.target_duration_sec.is_some() {
            return true;
        }

        if self.format_type.as_deref() == Some("FORMAT_STREAM_TYPE_OTF") {
            return true;
        }

        self.drm_families.is_some()
    }

    pub(crate) fn cipher_or_url(&self) -> Result<CipherParts, Error> {
        if let Some(url) = self.url.clone() {
            return Ok(CipherParts {
                url,
                sig: None,
                sp: None,
            });
        }

        let raw = self
            .signature_cipher
            .as_deref()
            .or(self.cipher.as_deref())
            .ok_or(Error::BadCipher)?;

        parse_cipher(raw)
    }

    pub(crate) fn to_stream(&self, url: String) -> Option<Stream> {
        let mime = self.mime_type.as_deref().unwrap_or("");
        let has_video = mime.starts_with("video/");
        let has_audio = mime.starts_with("audio/")
            || self.audio_quality.is_some()
            || codecs_have_audio(mime);

        if !has_video && !has_audio {
            return None;
        }

        let bitrate = self.average_bitrate.or(self.bitrate).unwrap_or(0);
        let height = self.height.unwrap_or(0);

        Some(Stream {
            url,
            height,
            bitrate,
            has_video,
            has_audio,
        })
    }

    pub(crate) fn duration_secs(&self) -> Option<f64> {
        json_f64(&self.approx_duration_ms).map(|ms| ms / 1000.0)
    }
}

pub(crate) struct CipherParts {
    pub url: String,
    pub sig: Option<String>,
    pub sp: Option<String>,
}

pub(crate) fn pick(streams: &[Stream]) -> Result<(String, Option<String>), Error> {
    let mut best_muxed: Option<&Stream> = None;

    for stream in streams {
        if !(stream.has_video && stream.has_audio) {
            continue;
        }

        if muxed_better(best_muxed, stream) {
            best_muxed = Some(stream);
        }
    }

    if let Some(stream) = best_muxed {
        return Ok((stream.url.clone(), None));
    }

    let mut best_video: Option<&Stream> = None;
    let mut best_audio: Option<&Stream> = None;

    for stream in streams {
        if stream.has_video && !stream.has_audio && video_better(best_video, stream) {
            best_video = Some(stream);
        }

        if stream.has_audio && !stream.has_video && audio_better(best_audio, stream) {
            best_audio = Some(stream);
        }
    }

    let Some(video) = best_video else {
        return Err(Error::NoFormats);
    };

    let Some(audio) = best_audio else {
        return Err(Error::NoFormats);
    };

    Ok((video.url.clone(), Some(audio.url.clone())))
}

fn muxed_better(best: Option<&Stream>, cand: &Stream) -> bool {
    let Some(best) = best else {
        return true;
    };

    if cand.height != best.height {
        return cand.height > best.height;
    }

    cand.bitrate > best.bitrate
}

fn video_better(best: Option<&Stream>, cand: &Stream) -> bool {
    muxed_better(best, cand)
}

fn audio_better(best: Option<&Stream>, cand: &Stream) -> bool {
    let Some(best) = best else {
        return true;
    };

    cand.bitrate > best.bitrate
}

fn codecs_have_audio(mime: &str) -> bool {
    let Some((_, codecs)) = mime.split_once("codecs=") else {
        return false;
    };

    let codecs = codecs.trim_matches('"');
    let has_mp4a = codecs.contains("mp4a");
    let has_opus = codecs.contains("opus");
    let has_vorbis = codecs.contains("vorbis");

    has_mp4a || has_opus || has_vorbis
}

fn parse_cipher(raw: &str) -> Result<CipherParts, Error> {
    let mut url = None;
    let mut sig = None;
    let mut sp = None;

    for pair in raw.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };

        let key = percent_decode(k);
        let val = percent_decode(v);

        if key == "url" {
            url = Some(val);
            continue;
        }

        if key == "s" {
            sig = Some(val);
            continue;
        }

        if key == "sp" {
            sp = Some(val);
        }
    }

    let Some(url) = url else {
        return Err(Error::BadCipher);
    };

    if sig.is_none() {
        return Err(Error::BadCipher);
    }

    Ok(CipherParts { url, sig, sp })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let is_pct = bytes[i] == b'%' && bytes.get(i + 2).is_some();

        if is_pct {
            let hi = hex_nibble(bytes[i + 1]);
            let lo = hex_nibble(bytes[i + 2]);

            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }

        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }

        out.push(bytes[i]);
        i += 1;
    }

    match String::from_utf8(out) {
        Ok(s) => s,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
    }
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn json_f64(v: &Option<serde_json::Value>) -> Option<f64> {
    let Some(v) = v else {
        return None;
    };

    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}
