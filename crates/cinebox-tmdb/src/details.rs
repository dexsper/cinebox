//! Movie/TV details and person combined credits (HTTP).

use cinebox_core::{CatalogItem, MediaDetails, MediaKind, PersonDetails, TmdbId};

use crate::catalog_map::catalog_items_from;
use crate::details_dto::{CollectionBody, MediaBody, PersonBody};
use crate::details_map::{MAX_ROW, media_from_body, person_from_body};
use crate::{API_BASE, Error, http_client, prepare_api_key, send_json};

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
        MediaKind::Person => return Err(Error::Http(400)),
    };

    let client = http_client(std::time::Duration::from_secs(25), use_system_proxy)?;
    let url = format!("{API_BASE}/{method}/{}", id.get());
    let mut request = client.get(&url).query(&[
        ("api_key", api_key),
        (
            "append_to_response",
            "credits,videos,recommendations,similar,release_dates,content_ratings",
        ),
    ]);

    if let Some(language) = language.filter(|s| !s.is_empty()) {
        request = request.query(&[("language", language)]);
    }

    let body: MediaBody = send_json(request).await?;
    let (mut details, collection_id) = media_from_body(body, kind, language)?;
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
    Ok(catalog_items_from(
        body.parts.unwrap_or_default(),
        Some(MediaKind::Movie),
        MAX_ROW,
    ))
}
