//! TMDB episode groups: the release numbering of shows TMDB splits differently
//! (Netflix "parts" against TMDB seasons).

use std::time::Duration;

use cinebox_core::TmdbId;
use cinebox_net::NetConfig;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::{API_BASE, Error, send_json};

const DIGITAL_GROUP_TYPE: u8 = 4;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

/// One file episode and the TMDB episode it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Renumber {
    pub season: u32,
    pub episode: u32,
    pub tmdb_season: u32,
    pub tmdb_episode: u32,
}

#[derive(Deserialize)]
struct GroupListBody {
    #[serde(default)]
    results: Vec<GroupSummary>,
}

#[derive(Deserialize)]
struct GroupSummary {
    id: String,
    #[serde(default, rename = "type")]
    kind: u8,
    #[serde(default)]
    group_count: u32,
}

#[derive(Deserialize)]
struct GroupBody {
    #[serde(default)]
    groups: Vec<GroupPart>,
}

#[derive(Deserialize)]
struct GroupPart {
    #[serde(default)]
    order: u32,
    #[serde(default)]
    episodes: Vec<GroupEpisode>,
}

#[derive(Deserialize)]
struct GroupEpisode {
    #[serde(default)]
    order: u32,
    #[serde(default)]
    season_number: u32,
    #[serde(default)]
    episode_number: u32,
}

/// Numbering of the first Digital episode group with a part for every
/// season in `wanted`, part N standing for season N. `None` when no group fits.
pub(crate) async fn digital_numbering(
    net: &NetConfig,
    api_key: &str,
    tv_id: TmdbId,
    wanted: &[u32],
) -> Result<Option<Vec<Renumber>>, Error> {
    let Some(last_season) = wanted.iter().copied().max() else {
        return Ok(None);
    };

    let list_url = format!("{API_BASE}/tv/{}/episode_groups", tv_id.get());
    let group_list: GroupListBody = fetch(net, api_key, &list_url).await?;

    let groups = group_list.results.into_iter();
    let candidates = groups.filter(|group| may_cover(group, last_season));

    for group in candidates {
        // The id goes into the URL path, so anything but a plain TMDB id is skipped.
        if !is_plain_id(&group.id) {
            continue;
        }

        let group_url = format!("{API_BASE}/tv/episode_group/{}", group.id);
        let body: GroupBody = fetch(net, api_key, &group_url).await?;

        if let Some(found) = numbering(body, wanted) {
            return Ok(Some(found));
        }
    }

    Ok(None)
}

async fn fetch<T: DeserializeOwned>(net: &NetConfig, api_key: &str, url: &str) -> Result<T, Error> {
    send_json(net, |client| client.get(url).timeout(REQUEST_TIMEOUT).query(&[("api_key", api_key)])).await
}

fn may_cover(group: &GroupSummary, last: u32) -> bool {
    // `group_count` also counts a specials part, so it only rules groups out.
    group.kind == DIGITAL_GROUP_TYPE && group.group_count >= last
}

fn is_plain_id(id: &str) -> bool {
    id.chars().all(|character| character.is_ascii_alphanumeric())
}

fn is_regular(episode: &GroupEpisode) -> bool {
    episode.season_number > 0
}

fn is_seasonal(part: &GroupPart) -> bool {
    // A part made only of TMDB specials is not a release season.
    part.episodes.iter().any(is_regular)
}

fn numbering(body: GroupBody, wanted: &[u32]) -> Option<Vec<Renumber>> {
    let seasonal = body.groups.into_iter().filter(is_seasonal);
    let mut parts: Vec<GroupPart> = seasonal.collect();
    parts.sort_unstable_by_key(|part| part.order);

    let mut out = Vec::new();

    for &season in wanted {
        let index = usize::try_from(season).ok()?.checked_sub(1)?;
        let part = parts.get_mut(index)?;
        part.episodes.sort_unstable_by_key(|episode| episode.order);

        let regular = part.episodes.iter().filter(|episode| is_regular(episode));
        for (episode, tmdb) in (1..).zip(regular) {
            out.push(Renumber { season, episode, tmdb_season: tmdb.season_number, tmdb_episode: tmdb.episode_number });
        }
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARTS: &str = r#"{
        "groups": [
            {
                "order": 2,
                "episodes": [
                    { "order": 1, "season_number": 1, "episode_number": 7 },
                    { "order": 0, "season_number": 1, "episode_number": 6 }
                ]
            },
            {
                "order": 0,
                "episodes": [{ "order": 0, "season_number": 0, "episode_number": 1 }]
            },
            {
                "order": 1,
                "episodes": [{ "order": 0, "season_number": 1, "episode_number": 1 }]
            },
            {
                "order": 3,
                "episodes": [
                    { "order": 0, "season_number": 3, "episode_number": 1 },
                    { "order": 1, "season_number": 3, "episode_number": 2 }
                ]
            }
        ]
    }"#;

    fn body(json: &str) -> GroupBody {
        serde_json::from_str(json).unwrap_or_else(|error| panic!("fixture: {error}"))
    }

    fn renumber(season: u32, episode: u32, tmdb_season: u32, tmdb_episode: u32) -> Renumber {
        Renumber { season, episode, tmdb_season, tmdb_episode }
    }

    #[test]
    fn maps_part_n_to_season_n_in_episode_order() {
        let mapped = numbering(body(PARTS), &[2, 3]);

        let expected = vec![renumber(2, 1, 1, 6), renumber(2, 2, 1, 7), renumber(3, 1, 3, 1), renumber(3, 2, 3, 2)];
        assert_eq!(mapped, Some(expected));
    }

    #[test]
    fn rejects_a_group_with_too_few_parts() {
        assert_eq!(numbering(body(PARTS), &[4]), None);
    }

    #[test]
    fn rejects_season_zero() {
        assert_eq!(numbering(body(PARTS), &[0]), None);
    }
}
