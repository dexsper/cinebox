use std::time::{Duration, Instant};

use cinebox_core::{join_url, normalize_base_url};
use futures_util::StreamExt;
use reqwest::StatusCode;

use super::client::{apply_basic_auth, http_client};
use super::error::Error;

/// Stop reading after 10s even if the generated file is larger.
const SPEED_TEST_MAX_SECS: u64 = 10;
const SPEED_TEST_MAX_BYTES: u64 = 300_000_000;
const MIN_SAMPLE_SECS: f64 = 0.25;

/// File size requested from TorrServer (`GET /download/{n}`).
pub const SPEED_TEST_FILE_MB: u32 = 300;

/// Result of `GET /download/{size}`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeedReport {
    pub size_mb: u32,
    pub bytes: u64,
    pub elapsed: Duration,
}

impl SpeedReport {
    /// Throughput in megabits per second.
    #[must_use]
    pub fn megabits_per_sec(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64().max(f64::EPSILON);
        (self.bytes as f64) * 8.0 / secs / 1_000_000.0
    }

    /// Short UI line, no URLs.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{:.1} Mbps ({} MB source, {:.1}s, {} KB)",
            self.megabits_per_sec(),
            self.size_mb,
            self.elapsed.as_secs_f64(),
            self.bytes / 1024
        )
    }
}

/// Progress while a speed test is running.
#[derive(Clone, Copy, Debug)]
pub enum SpeedEvent {
    Testing,
    Sample {
        mbps: f64,
        elapsed: Duration,
        bytes: u64,
    },
}

/// `GET /echo` unauthenticated version string (ping).
///
/// # Errors
///
/// Empty URL, HTTP failures, or an empty body.
pub async fn echo(base_url: &str, username: &str, password: &str) -> Result<String, Error> {
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    let url = join_url(&base, "echo");
    let client = http_client()?;
    let get = client.get(&url).timeout(Duration::from_secs(10));
    let response = apply_basic_auth(get, username, password)
        .send()
        .await
        .map_err(Error::Request)?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }

    let body = response.text().await.map_err(Error::Request)?;
    let version = body.trim();
    if version.is_empty() {
        return Err(Error::EmptyEcho);
    }

    Ok(version.to_owned())
}

/// `GET /download/{n}` with a 10s read cap. Needs Basic auth when the server has `HttpAuth`.
///
/// # Errors
///
/// Empty URL, HTTP failures, or zero bytes read.
pub async fn speed_test(
    base_url: &str,
    username: &str,
    password: &str,
    mut on_event: impl FnMut(SpeedEvent) + Send,
) -> Result<SpeedReport, Error> {
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    let path = format!("download/{SPEED_TEST_FILE_MB}");
    let url = join_url(&base, &path);
    let client = http_client()?;
    let get = client.get(&url).timeout(Duration::from_secs(20));
    let response = apply_basic_auth(get, username, password)
        .send()
        .await
        .map_err(Error::Request)?;

    if response.status() == StatusCode::UNAUTHORIZED {
        return Err(Error::Http(401));
    }
    
    if !response.status().is_success() {
        return Err(Error::Http(response.status().as_u16()));
    }

    let started = Instant::now();
    let mut bytes = 0_u64;
    let mut testing = false;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(Error::Request)?;
        bytes += chunk.len() as u64;

        if !testing {
            testing = true;
            on_event(SpeedEvent::Testing);
        }

        let elapsed = started.elapsed();
        let secs = elapsed.as_secs_f64();
        let warmed_up = secs >= MIN_SAMPLE_SECS;
        if warmed_up {
            let mbps = (bytes as f64) * 8.0 / secs / 1_000_000.0;
            on_event(SpeedEvent::Sample {
                mbps,
                elapsed,
                bytes,
            });
        }

        if started.elapsed() >= Duration::from_secs(SPEED_TEST_MAX_SECS) {
            break;
        }
        if bytes >= SPEED_TEST_MAX_BYTES {
            break;
        }
    }
    if bytes == 0 {
        return Err(Error::NoData);
    }

    Ok(SpeedReport {
        size_mb: SPEED_TEST_FILE_MB,
        bytes,
        elapsed: started.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mbps_ten_megabytes_in_eight_seconds() {
        let report = SpeedReport {
            size_mb: 10,
            bytes: 10_000_000,
            elapsed: Duration::from_secs(8),
        };
        let mbps = report.megabits_per_sec();
        assert!((mbps - 10.0).abs() < 0.01, "{mbps}");
    }
}
