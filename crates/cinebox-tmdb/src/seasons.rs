//! TMDB `GET /tv/{id}/season/{n}` for episode names and stills.

use cinebox_core::TmdbId;
use serde::{Deserialize, Serialize};

use crate::{API_BASE, Error, http_client, prepare_api_key, send_json};

/// One episode from a season payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonEpisode {
    pub season: u32,
    pub episode: u32,
    pub name: String,
    pub still_path: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub air_date: Option<String>,
}

#[derive(Deserialize)]
struct SeasonBody {
    #[serde(default)]
    season_number: Option<u32>,
    #[serde(default)]
    episodes: Option<Vec<EpisodeBody>>,
}

#[derive(Deserialize)]
struct EpisodeBody {
    #[serde(default)]
    episode_number: Option<u32>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    still_path: Option<String>,
    #[serde(default)]
    runtime: Option<u32>,
    #[serde(default)]
    air_date: Option<String>,
}

/// Fetch episode stills/names for the given season numbers.
///
/// # Errors
///
/// Empty key or HTTP/JSON failures. Missing seasons are skipped.
pub async fn fetch_season_episodes(
    api_key: &str,
    tv_id: TmdbId,
    seasons: &[u32],
    language: Option<&str>,
    use_system_proxy: bool,
) -> Result<Vec<SeasonEpisode>, Error> {
    let api_key = prepare_api_key(api_key)?;
    let client = http_client(std::time::Duration::from_secs(25), use_system_proxy)?;
    let mut out = Vec::new();
    let mut seen = Vec::new();

    for season in seasons {
        if *season == 0 || seen.contains(season) {
            continue;
        }
        seen.push(*season);
        let url = format!("{API_BASE}/tv/{}/season/{season}", tv_id.get());
        let mut request = client.get(&url).query(&[("api_key", api_key)]);
        if let Some(language) = language.filter(|s| !s.is_empty()) {
            request = request.query(&[("language", language)]);
        }

        let body: SeasonBody = match send_json(request).await {
            Ok(body) => body,
            Err(_) => continue,
        };

        let season_no = body.season_number.unwrap_or(*season);
        for ep in body.episodes.unwrap_or_default() {
            let Some(episode) = ep.episode_number.filter(|n| *n > 0) else {
                continue;
            };
            out.push(SeasonEpisode {
                season: season_no,
                episode,
                name: ep.name.unwrap_or_default(),
                still_path: ep.still_path.filter(|p| !p.is_empty()),
                runtime_minutes: ep.runtime.filter(|n| *n > 0),
                air_date: ep.air_date.filter(|d| !d.is_empty()),
            });
        }
    }
    Ok(out)
}
