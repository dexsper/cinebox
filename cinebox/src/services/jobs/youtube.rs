//! Trailer streams from YouTube.

use cinebox_net::NetConfig;
use tracing::warn;

use super::JobError;

const YOUTUBE_ORIGIN: &str = "https://www.youtube.com";

pub async fn resolve_youtube(
    key: String,
    net: NetConfig,
) -> Result<cinebox_youtube::Playback, JobError> {
    let id = cinebox_youtube::VideoId::parse(&key)?;
    tracing::debug!(
        id = id.as_str(),
        proxy = net.use_system_proxy,
        dns_bypass = net.dns_bypass,
        "youtube resolve job"
    );

    let play = match cinebox_youtube::resolve(YOUTUBE_ORIGIN, &id, &net).await {
        Ok(play) => play,
        Err(error) => {
            warn!(%error, id = id.as_str(), "youtube resolve failed");
            return Err(error.into());
        }
    };

    tracing::debug!(
        id = id.as_str(),
        has_audio = play.audio_url.is_some(),
        "youtube resolve ok"
    );

    Ok(play)
}
