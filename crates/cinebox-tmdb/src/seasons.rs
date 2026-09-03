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

/// Fetch episode stills/names for the given season numbers, in parallel.
///
/// # Errors
///
/// Empty key. A failed season is logged and skipped instead of failing the
/// whole batch.
pub async fn fetch_season_episodes(
    api_key: &str,
    tv_id: TmdbId,
    seasons: &[u32],
    language: Option<&str>,
    use_system_proxy: bool,
) -> Result<Vec<SeasonEpisode>, Error> {
    let api_key = prepare_api_key(api_key)?;
    let client = http_client(use_system_proxy)?;

    let mut wanted: Vec<u32> = seasons.iter().copied().filter(|s| *s > 0).collect();
    wanted.sort_unstable();
    wanted.dedup();

    let fetches = wanted
        .iter()
        .map(|season| fetch_one_season(&client, api_key, tv_id, *season, language));

    let mut out = Vec::new();
    let results = futures_util::future::join_all(fetches).await;

    for (season, result) in wanted.iter().zip(results) {
        match result {
            Ok(episodes) => out.extend(episodes),
            Err(error) => tracing::warn!(%error, season, "tmdb season fetch failed"),
        }
    }

    Ok(out)
}

async fn fetch_one_season(
    client: &reqwest::Client,
    api_key: &str,
    tv_id: TmdbId,
    season: u32,
    language: Option<&str>,
) -> Result<Vec<SeasonEpisode>, Error> {
    let url = format!("{API_BASE}/tv/{}/season/{season}", tv_id.get());
    let mut request = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(25))
        .query(&[("api_key", api_key)]);

    if let Some(language) = language.filter(|s| !s.is_empty()) {
        request = request.query(&[("language", language)]);
    }

    let body: SeasonBody = send_json(request).await?;

    Ok(map_season(body, season))
}

/// Pure payload → episodes mapping; `requested` backs a missing `season_number`.
fn map_season(body: SeasonBody, requested: u32) -> Vec<SeasonEpisode> {
    let season = body.season_number.unwrap_or(requested);
    let mut out = Vec::new();

    for ep in body.episodes.unwrap_or_default() {
        let Some(episode) = ep.episode_number.filter(|n| *n > 0) else {
            continue;
        };

        out.push(SeasonEpisode {
            season,
            episode,
            name: ep.name.unwrap_or_default(),
            still_path: ep.still_path.filter(|p| !p.is_empty()),
            runtime_minutes: ep.runtime.filter(|n| *n > 0),
            air_date: ep.air_date.filter(|d| !d.is_empty()),
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: &str) -> SeasonBody {
        serde_json::from_str(json).unwrap_or_else(|error| panic!("fixture: {error}"))
    }

    #[test]
    fn maps_full_tmdb_payload() {
        let fixture = r#"{
            "season_number": 2,
            "episodes": [
                {
                    "episode_number": 1,
                    "name": "Pilot",
                    "still_path": "/abc.jpg",
                    "runtime": 52,
                    "air_date": "2024-01-15"
                },
                {
                    "episode_number": 2,
                    "name": "Fallout"
                }
            ]
        }"#;

        let episodes = map_season(body(fixture), 9);

        assert_eq!(
            episodes,
            vec![
                SeasonEpisode {
                    season: 2,
                    episode: 1,
                    name: String::from("Pilot"),
                    still_path: Some(String::from("/abc.jpg")),
                    runtime_minutes: Some(52),
                    air_date: Some(String::from("2024-01-15")),
                },
                SeasonEpisode {
                    season: 2,
                    episode: 2,
                    name: String::from("Fallout"),
                    still_path: None,
                    runtime_minutes: None,
                    air_date: None,
                },
            ]
        );
    }

    #[test]
    fn falls_back_to_requested_season() {
        let fixture = r#"{ "episodes": [{ "episode_number": 3 }] }"#;
        let episodes = map_season(body(fixture), 7);

        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].season, 7);
        assert_eq!(episodes[0].episode, 3);
    }

    #[test]
    fn skips_missing_or_zero_episode_numbers() {
        let fixture = r#"{
            "season_number": 1,
            "episodes": [
                { "name": "no number" },
                { "episode_number": 0, "name": "special" },
                { "episode_number": 4, "name": "kept" }
            ]
        }"#;

        let episodes = map_season(body(fixture), 1);

        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].episode, 4);
    }

    #[test]
    fn empty_strings_and_zero_runtime_become_none() {
        let fixture = r#"{
            "season_number": 1,
            "episodes": [
                { "episode_number": 1, "still_path": "", "runtime": 0, "air_date": "" }
            ]
        }"#;

        let episodes = map_season(body(fixture), 1);

        assert_eq!(episodes[0].still_path, None);
        assert_eq!(episodes[0].runtime_minutes, None);
        assert_eq!(episodes[0].air_date, None);
    }

    #[test]
    fn empty_payload_maps_to_no_episodes() {
        let episodes = map_season(body("{}"), 5);

        assert!(episodes.is_empty());
    }
}
