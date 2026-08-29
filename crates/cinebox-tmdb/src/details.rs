//! Movie/TV details and person combined credits.

use cinebox_core::{
    CatalogItem, CreditPerson, MediaDetails, MediaKind, PersonDetails, TmdbId, Trailer,
    year_from_date,
};
use serde::Deserialize;

use crate::{API_BASE, Error, http_client, into_request, prepare_api_key};

const MAX_CAST: usize = 20;
const MAX_ROW: usize = 20;
const MAX_PERSON_CREDITS: usize = 40;

#[derive(Deserialize)]
struct Named {
    name: Option<String>,
}

#[derive(Deserialize)]
struct Country {
    name: Option<String>,
}

#[derive(Deserialize)]
struct CollectionRef {
    id: Option<u32>,
}

#[derive(Deserialize)]
struct CreditsBlock {
    cast: Option<Vec<CreditRaw>>,
    crew: Option<Vec<CreditRaw>>,
}

#[derive(Deserialize)]
struct CreditRaw {
    id: Option<u32>,
    name: Option<String>,
    character: Option<String>,
    job: Option<String>,
    profile_path: Option<String>,
    order: Option<u32>,
}

#[derive(Deserialize)]
struct VideosBlock {
    results: Option<Vec<VideoRaw>>,
}

#[derive(Deserialize)]
struct VideoRaw {
    name: Option<String>,
    key: Option<String>,
    site: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct ListBlock {
    results: Option<Vec<ListRaw>>,
}

#[derive(Deserialize)]
struct ListRaw {
    id: Option<u32>,
    title: Option<String>,
    name: Option<String>,
    poster_path: Option<String>,
    vote_average: Option<f32>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    media_type: Option<String>,
}

#[derive(Deserialize)]
struct CollectionBody {
    parts: Option<Vec<ListRaw>>,
}

#[derive(Deserialize)]
struct MediaBody {
    id: Option<u32>,
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    tagline: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    runtime: Option<u32>,
    episode_run_time: Option<Vec<u32>>,
    vote_average: Option<f32>,
    budget: Option<u64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    genres: Option<Vec<Named>>,
    production_countries: Option<Vec<Country>>,
    origin_country: Option<Vec<String>>,
    belongs_to_collection: Option<CollectionRef>,
    credits: Option<CreditsBlock>,
    videos: Option<VideosBlock>,
    recommendations: Option<ListBlock>,
    similar: Option<ListBlock>,
}

#[derive(Deserialize)]
struct CombinedCredits {
    cast: Option<Vec<ListRaw>>,
    crew: Option<Vec<ListRaw>>,
}

#[derive(Deserialize)]
struct PersonBody {
    id: Option<u32>,
    name: Option<String>,
    biography: Option<String>,
    birthday: Option<String>,
    place_of_birth: Option<String>,
    profile_path: Option<String>,
    combined_credits: Option<CombinedCredits>,
}

pub async fn fetch_media(
    api_key: &str,
    kind: MediaKind,
    id: TmdbId,
    language: Option<&str>,
    use_system_proxy: bool,
) -> Result<MediaDetails, Error> {
    let api_key = prepare_api_key(api_key)?;
    let method = match kind {
        MediaKind::Movie => "movie",
        MediaKind::Tv => "tv",
        MediaKind::Person => {
            return Err(Error::Http(400));
        }
    };
    let client = http_client(std::time::Duration::from_secs(25), use_system_proxy)?;
    let url = format!("{API_BASE}/{method}/{}", id.get());
    let mut request = client.get(&url).query(&[
        ("api_key", api_key),
        (
            "append_to_response",
            "credits,videos,recommendations,similar",
        ),
    ]);
    if let Some(language) = language.filter(|s| !s.is_empty()) {
        request = request.query(&[("language", language)]);
    }
    let body: MediaBody = send_json(request).await?;
    let (mut details, collection_id) = media_from_body(body, kind)?;
    if let Some(col_id) = collection_id
        && let Ok(items) = fetch_collection(&client, api_key, col_id, language).await
    {
        details.collection = items;
    }
    Ok(details)
}

pub async fn fetch_person(
    api_key: &str,
    id: TmdbId,
    language: Option<&str>,
    use_system_proxy: bool,
) -> Result<PersonDetails, Error> {
    let api_key = prepare_api_key(api_key)?;
    let client = http_client(std::time::Duration::from_secs(25), use_system_proxy)?;
    let url = format!("{API_BASE}/person/{}", id.get());
    let mut request = client.get(&url).query(&[
        ("api_key", api_key),
        ("append_to_response", "combined_credits"),
    ]);
    if let Some(language) = language.filter(|s| !s.is_empty()) {
        request = request.query(&[("language", language)]);
    }
    let body: PersonBody = send_json(request).await?;
    person_from_body(body)
}

async fn send_json<T: serde::de::DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> Result<T, Error> {
    let response = request.send().await.map_err(into_request)?;
    let status = response.status();
    if status.as_u16() == 401 {
        return Err(Error::Unauthorized);
    }
    if !status.is_success() {
        return Err(Error::Http(status.as_u16()));
    }
    response.json().await.map_err(into_request)
}

async fn fetch_collection(
    client: &reqwest::Client,
    api_key: &str,
    id: u32,
    language: Option<&str>,
) -> Result<Vec<CatalogItem>, Error> {
    let url = format!("{API_BASE}/collection/{id}");
    let mut request = client.get(&url).query(&[("api_key", api_key)]);
    if let Some(language) = language.filter(|s| !s.is_empty()) {
        request = request.query(&[("language", language)]);
    }
    let body: CollectionBody = send_json(request).await?;
    Ok(items_from_raw(
        &body.parts.unwrap_or_default(),
        Some(MediaKind::Movie),
        MAX_ROW,
    ))
}

fn media_from_body(body: MediaBody, kind: MediaKind) -> Result<(MediaDetails, Option<u32>), Error> {
    let Some(id) = body.id.filter(|id| *id > 0) else {
        return Err(Error::Http(404));
    };
    let title = match kind {
        MediaKind::Movie => body.title.or(body.name),
        MediaKind::Tv => body.name.or(body.title),
        MediaKind::Person => None,
    }
    .filter(|t| !t.is_empty())
    .ok_or(Error::Http(404))?;
    let original = match kind {
        MediaKind::Movie => body.original_title.or(body.original_name),
        MediaKind::Tv => body.original_name.or(body.original_title),
        MediaKind::Person => None,
    }
    .filter(|t| !t.is_empty() && *t != title);
    let date = match kind {
        MediaKind::Movie => body.release_date.as_deref(),
        MediaKind::Tv => body.first_air_date.as_deref(),
        MediaKind::Person => None,
    };
    let runtime = match kind {
        MediaKind::Movie => body.runtime.filter(|m| *m > 0),
        MediaKind::Tv => body
            .episode_run_time
            .as_ref()
            .and_then(|v| v.iter().copied().find(|m| *m > 0)),
        MediaKind::Person => None,
    };
    let countries = if kind == MediaKind::Tv {
        body.origin_country
            .unwrap_or_default()
            .into_iter()
            .filter(|c| !c.is_empty())
            .collect()
    } else {
        names(
            body.production_countries
                .unwrap_or_default()
                .into_iter()
                .filter_map(|c| c.name),
        )
    };
    let credits = body.credits.unwrap_or(CreditsBlock {
        cast: None,
        crew: None,
    });
    let directors = credits
        .crew
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.job.as_deref() == Some("Director"))
        .filter_map(|c| credit_person(c, true))
        .collect();
    let mut cast: Vec<CreditPerson> = credits
        .cast
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| credit_person(c, false))
        .collect();
    cast.truncate(MAX_CAST);
    let collection_id = body
        .belongs_to_collection
        .and_then(|c| c.id.filter(|id| *id > 0));
    Ok((
        MediaDetails {
            id: TmdbId::new(id),
            kind,
            title,
            original_title: original,
            tagline: nonempty(body.tagline),
            overview: nonempty(body.overview),
            year: date.and_then(year_from_date),
            released: date.and_then(|d| nonempty(Some(d.to_owned()))),
            runtime_minutes: runtime,
            vote: body.vote_average.filter(|v| *v > 0.0),
            budget: body.budget.filter(|b| *b > 0),
            genres: names(
                body.genres
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|g| g.name),
            ),
            countries,
            poster_path: nonempty(body.poster_path),
            backdrop_path: nonempty(body.backdrop_path),
            directors,
            cast,
            collection: Vec::new(),
            recommendations: items_from_raw(
                &body
                    .recommendations
                    .and_then(|b| b.results)
                    .unwrap_or_default(),
                Some(kind),
                MAX_ROW,
            ),
            similar: items_from_raw(
                &body.similar.and_then(|b| b.results).unwrap_or_default(),
                Some(kind),
                MAX_ROW,
            ),
            trailers: trailers_from(body.videos.and_then(|v| v.results).unwrap_or_default()),
        },
        collection_id,
    ))
}

fn names(iter: impl Iterator<Item = String>) -> Vec<String> {
    iter.filter(|s| !s.is_empty()).collect()
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}

fn credit_person(raw: CreditRaw, crew: bool) -> Option<CreditPerson> {
    let id = raw.id.filter(|id| *id > 0)?;
    let name = raw.name.filter(|n| !n.is_empty())?;
    let role = if crew {
        raw.job.unwrap_or_default()
    } else {
        raw.character.unwrap_or_default()
    };
    let _ = raw.order;
    Some(CreditPerson {
        id: TmdbId::new(id),
        name,
        role,
        profile_path: nonempty(raw.profile_path),
    })
}

fn trailers_from(videos: Vec<VideoRaw>) -> Vec<Trailer> {
    let mut out = Vec::new();
    for video in videos {
        if video.site.as_deref() != Some("YouTube") {
            continue;
        }
        let kind = video.kind.as_deref().unwrap_or("");
        if kind != "Trailer" && kind != "Teaser" {
            continue;
        }
        let Some(key) = video.key.filter(|k| !k.is_empty()) else {
            continue;
        };
        let name = video
            .name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| String::from(kind));
        out.push(Trailer {
            name,
            youtube_key: key,
        });
    }
    out
}

fn items_from_raw(items: &[ListRaw], fallback: Option<MediaKind>, cap: usize) -> Vec<CatalogItem> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for raw in items {
        if out.len() >= cap {
            break;
        }
        let Some(id) = raw.id.filter(|id| *id > 0) else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        let kind = raw.media_type.as_deref().and_then(kind_from).or(fallback);
        let Some(kind @ (MediaKind::Movie | MediaKind::Tv)) = kind else {
            continue;
        };
        let title = match kind {
            MediaKind::Movie => raw.title.clone().or_else(|| raw.name.clone()),
            MediaKind::Tv => raw.name.clone().or_else(|| raw.title.clone()),
            MediaKind::Person => None,
        };
        let Some(title) = title.filter(|t| !t.is_empty()) else {
            continue;
        };
        let date = match kind {
            MediaKind::Movie => raw.release_date.as_deref(),
            MediaKind::Tv => raw.first_air_date.as_deref(),
            MediaKind::Person => None,
        };
        seen.push(id);
        out.push(CatalogItem {
            id: TmdbId::new(id),
            kind,
            title,
            year: date.and_then(year_from_date),
            vote: raw.vote_average.filter(|v| *v > 0.0),
            poster_path: nonempty(raw.poster_path.clone()),
        });
    }
    out
}

fn kind_from(value: &str) -> Option<MediaKind> {
    match value {
        "movie" => Some(MediaKind::Movie),
        "tv" => Some(MediaKind::Tv),
        _ => None,
    }
}

fn person_from_body(body: PersonBody) -> Result<PersonDetails, Error> {
    let Some(id) = body.id.filter(|id| *id > 0) else {
        return Err(Error::Http(404));
    };
    let name = body
        .name
        .filter(|n| !n.is_empty())
        .ok_or(Error::Http(404))?;
    let credits_raw = body.combined_credits.unwrap_or(CombinedCredits {
        cast: None,
        crew: None,
    });
    let mut combined = credits_raw.cast.unwrap_or_default();
    combined.extend(credits_raw.crew.unwrap_or_default());
    Ok(PersonDetails {
        id: TmdbId::new(id),
        name,
        biography: nonempty(body.biography),
        birthday: nonempty(body.birthday),
        place_of_birth: nonempty(body.place_of_birth),
        profile_path: nonempty(body.profile_path),
        credits: items_from_raw(&combined, None, MAX_PERSON_CREDITS),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_youtube_trailers_only() {
        let videos = vec![
            VideoRaw {
                name: Some(String::from("Official")),
                key: Some(String::from("abc")),
                site: Some(String::from("YouTube")),
                kind: Some(String::from("Trailer")),
            },
            VideoRaw {
                name: Some(String::from("Vimeo")),
                key: Some(String::from("no")),
                site: Some(String::from("Vimeo")),
                kind: Some(String::from("Trailer")),
            },
        ];
        let trailers = trailers_from(videos);
        assert_eq!(trailers.len(), 1);
        assert_eq!(
            trailers[0].watch_url(),
            "https://www.youtube.com/watch?v=abc"
        );
    }

    #[test]
    fn movie_json_maps_to_details() {
        let json = r#"{
            "id": 42,
            "title": "Dune",
            "original_title": "Dune: Part One",
            "overview": "Sand.",
            "release_date": "2021-10-22",
            "runtime": 155,
            "vote_average": 8.1,
            "budget": 165000000,
            "poster_path": "/x.jpg",
            "genres": [{"name": "Sci-Fi"}],
            "production_countries": [{"name": "United States"}],
            "credits": {
                "cast": [{"id": 1, "name": "Tim", "character": "Paul", "order": 0}],
                "crew": [{"id": 2, "name": "Denis", "job": "Director"}]
            },
            "videos": {"results": [{"name": "Official", "key": "abc", "site": "YouTube", "type": "Trailer"}]},
            "recommendations": {"results": [{"id": 9, "title": "Other", "media_type": "movie"}]},
            "similar": {"results": []},
            "belongs_to_collection": {"id": 77}
        }"#;
        let body = match serde_json::from_str::<MediaBody>(json) {
            Ok(body) => body,
            Err(error) => panic!("{error}"),
        };
        let (details, collection_id) = match media_from_body(body, MediaKind::Movie) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(details.id.get(), 42);
        assert_eq!(details.title, "Dune");
        assert_eq!(details.original_title.as_deref(), Some("Dune: Part One"));
        assert_eq!(details.year, Some(2021));
        assert_eq!(details.released.as_deref(), Some("2021-10-22"));
        assert_eq!(details.directors.len(), 1);
        assert_eq!(details.cast[0].role, "Paul");
        assert_eq!(details.trailers.len(), 1);
        assert_eq!(details.recommendations.len(), 1);
        assert_eq!(collection_id, Some(77));
    }

    #[test]
    fn person_json_skips_person_credits() {
        let json = r#"{
            "id": 5,
            "name": "Denis",
            "biography": "Director.",
            "combined_credits": {
                "cast": [{"id": 1, "title": "Dune", "media_type": "movie"}],
                "crew": [{"id": 2, "name": "Someone", "media_type": "person"}]
            }
        }"#;
        let body = match serde_json::from_str::<PersonBody>(json) {
            Ok(body) => body,
            Err(error) => panic!("{error}"),
        };
        let details = match person_from_body(body) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(details.credits.len(), 1);
        assert_eq!(details.credits[0].title, "Dune");
    }
}
