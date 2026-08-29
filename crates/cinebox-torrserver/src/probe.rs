//! Phase 2: `GET /echo` and `GET /download/{size}`.

use std::time::{Duration, Instant};

use cinebox_core::{join_url, normalize_base_url};
use futures_util::StreamExt;
use reqwest::StatusCode;

use super::client::{apply_basic_auth, http_client};
use super::error::Error;

/// Cap like Lampa: stop reading after 10s even if the generated file is larger.
const SPEED_TEST_MAX_SECS: u64 = 10;
const SPEED_TEST_MAX_BYTES: u64 = 300_000_000;

/// Sizes offered in settings (MB). Lampa uses 300 MB but aborts at 10s.
pub const SPEED_TEST_SIZES_MB: [u32; 3] = [10, 50, 100];

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

/// `GET /echo` — unauthenticated version string (ping).
///
/// # Errors
///
/// Empty URL, HTTP failures, or an empty body.
pub async fn echo(base_url: &str, username: &str, password: &str) -> Result<String, Error> {
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    let url = join_url(&base, "echo");
    let client = http_client(Duration::from_secs(10))?;
    let response = apply_basic_auth(client.get(&url), username, password)
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

/// `GET /download/{size}` with a 10s read cap. Needs Basic auth when the server has `HttpAuth`.
///
/// # Errors
///
/// Empty URL, bad size, HTTP failures, or zero bytes read.
pub async fn speed_test(
    base_url: &str,
    username: &str,
    password: &str,
    size_mb: u32,
) -> Result<SpeedReport, Error> {
    if !(1..=100).contains(&size_mb) {
        return Err(Error::BadSize);
    }
    let base = normalize_base_url(base_url).map_err(|_| Error::EmptyUrl)?;
    let url = join_url(&base, &format!("download/{size_mb}"));
    let client = http_client(Duration::from_secs(20))?;
    let response = apply_basic_auth(client.get(&url), username, password)
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
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(Error::Request)?;
        bytes += chunk.len() as u64;
        if started.elapsed() >= Duration::from_secs(SPEED_TEST_MAX_SECS)
            || bytes >= SPEED_TEST_MAX_BYTES
        {
            break;
        }
    }
    if bytes == 0 {
        return Err(Error::NoData);
    }
    Ok(SpeedReport {
        size_mb,
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

    #[tokio::test]
    #[ignore = "needs a local TorrServer on 127.0.0.1:8090"]
    async fn live_echo_localhost() -> Result<(), Error> {
        let version = echo("http://127.0.0.1:8090", "", "").await?;
        assert!(!version.is_empty(), "{version}");
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs a local TorrServer on 127.0.0.1:8090"]
    async fn live_speed_test_10mb() -> Result<(), Error> {
        let report = speed_test("http://127.0.0.1:8090", "", "", 10).await?;
        assert!(report.bytes > 0);
        assert!(report.megabits_per_sec() > 0.0);
        Ok(())
    }
}
