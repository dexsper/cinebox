//! TMDB `GET /tv/{id}/season/{n}` for episode names and stills.

use cinebox_core::TmdbId;
use cinebox_net::NetConfig;
use serde::{Deserialize, Serialize};

use crate::episode_groups::{Renumber, digital_numbering};
use crate::{API_BASE, Error, prepare_api_key, send_json};

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
/// When TMDB lacks one of the seasons, the release is taken to count seasons
/// the way the show's Digital episode group does, and every season is
/// renumbered through it.
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
    net: &NetConfig,
) -> Result<Vec<SeasonEpisode>, Error> {
    let api_key = prepare_api_key(api_key)?;

    let mut wanted: Vec<u32> = seasons.iter().copied().filter(|s| *s > 0).collect();
    wanted.sort_unstable();
    wanted.dedup();

    let results = fetch_seasons(net, api_key, tv_id, &wanted, language).await;

    let is_missing = results
        .iter()
        .any(|(_, result)| matches!(result, Err(Error::Http(404))));

    if is_missing {
        match regrouped(net, api_key, tv_id, &wanted, language).await {
            Ok(Some(episodes)) => return Ok(episodes),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "tmdb episode group fetch failed"),
        }
    }

    Ok(fetched_episodes(results))
}

async fn fetch_seasons(
    net: &NetConfig,
    api_key: &str,
    tv_id: TmdbId,
    seasons: &[u32],
    language: Option<&str>,
) -> Vec<(u32, Result<Vec<SeasonEpisode>, Error>)> {
    let fetches = seasons
        .iter()
        .map(|season| fetch_one_season(net, api_key, tv_id, *season, language));

    let results = futures_util::future::join_all(fetches).await;
    seasons.iter().copied().zip(results).collect()
}

fn fetched_episodes(results: Vec<(u32, Result<Vec<SeasonEpisode>, Error>)>) -> Vec<SeasonEpisode> {
    let mut out = Vec::new();

    for (season, result) in results {
        match result {
            Ok(episodes) => out.extend(episodes),
            Err(error) => tracing::warn!(%error, season, "tmdb season fetch failed"),
        }
    }

    out
}

async fn regrouped(
    net: &NetConfig,
    api_key: &str,
    tv_id: TmdbId,
    wanted: &[u32],
    language: Option<&str>,
) -> Result<Option<Vec<SeasonEpisode>>, Error> {
    let Some(numbering) = digital_numbering(net, api_key, tv_id, wanted).await? else {
        return Ok(None);
    };

    let mut tmdb_seasons: Vec<u32> = numbering
        .iter()
        .map(|renumber| renumber.tmdb_season)
        .collect();

    tmdb_seasons.sort_unstable();
    tmdb_seasons.dedup();

    // Names and stills come from the seasons, not the group: TMDB never translates a group payload.
    let results = fetch_seasons(net, api_key, tv_id, &tmdb_seasons, language).await;
    let catalog = fetched_episodes(results);

    Ok(Some(renumbered(&numbering, &catalog)))
}

fn renumbered(numbering: &[Renumber], catalog: &[SeasonEpisode]) -> Vec<SeasonEpisode> {
    let mut out = Vec::with_capacity(numbering.len());

    for renumber in numbering {
        let target = (renumber.tmdb_season, renumber.tmdb_episode);
        let found = catalog
            .iter()
            .find(|episode| (episode.season, episode.episode) == target);

        let Some(episode) = found else { continue };

        out.push(SeasonEpisode {
            season: renumber.season,
            episode: renumber.episode,
            ..episode.clone()
        });
    }

    out
}

async fn fetch_one_season(
    net: &NetConfig,
    api_key: &str,
    tv_id: TmdbId,
    season: u32,
    language: Option<&str>,
) -> Result<Vec<SeasonEpisode>, Error> {
    let url = format!("{API_BASE}/tv/{}/season/{season}", tv_id.get());
    let body: SeasonBody = send_json(net, |client| {
        let mut request = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(25))
            .query(&[("api_key", api_key)]);

        if let Some(language) = language.filter(|s| !s.is_empty()) {
            request = request.query(&[("language", language)]);
        }

        request
    })
    .await?;

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
    fn renumbers_tmdb_episodes_into_release_numbering() {
        let named = |season, episode, name: &str| SeasonEpisode {
            season,
            episode,
            name: String::from(name),
            still_path: None,
            runtime_minutes: None,
            air_date: None,
        };
        let catalog = vec![named(3, 1, "first"), named(3, 2, "second")];
        let into_part_4 = |episode, tmdb_episode| Renumber {
            season: 4,
            episode,
            tmdb_season: 3,
            tmdb_episode,
        };
        let numbering = [into_part_4(1, 1), into_part_4(2, 9), into_part_4(3, 2)];

        let episodes = renumbered(&numbering, &catalog);

        assert_eq!(episodes, vec![named(4, 1, "first"), named(4, 3, "second")]);
    }

    #[test]
    fn empty_payload_maps_to_no_episodes() {
        let episodes = map_season(body("{}"), 5);

        assert!(episodes.is_empty());
    }
}
