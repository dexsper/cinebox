//! Movie/TV details and person combined credits (HTTP).

use cinebox_core::{CatalogItem, MediaDetails, MediaKind, PersonDetails, TmdbId};
use cinebox_net::NetConfig;

use crate::catalog_map::catalog_items_from;
use crate::details_dto::{CollectionBody, MediaBody, PersonBody};
use crate::details_map::{MAX_ROW, media_from_body, person_from_body};
use crate::{Api, Error, send_json};

const DETAILS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25);
const MEDIA_APPENDS: &str = "credits,videos,recommendations,similar,release_dates,content_ratings";

pub(crate) async fn fetch_media(
    api: &Api,
    kind: MediaKind,
    id: TmdbId,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<MediaDetails, Error> {
    let method = match kind {
        MediaKind::Movie => "movie",
        MediaKind::Tv => "tv",
        MediaKind::Person => return Err(Error::UnsupportedKind),
    };

    let url = api.url(&format!("{method}/{}", id.get()));
    let body: MediaBody = send_json(net, |client| {
        let builder = api.get(client, &url).timeout(DETAILS_TIMEOUT);
        let mut request = builder.query(&[("append_to_response", MEDIA_APPENDS)]);

        if let Some(language) = language.filter(|s| !s.is_empty()) {
            request = request.query(&[("language", language)]);
        }

        request
    })
    .await?;

    let (mut details, collection_id) = media_from_body(body, kind, language)?;
    if let Some(col_id) = collection_id {
        match fetch_collection(net, api, col_id, language).await {
            Ok(items) => details.collection = items,
            Err(error) => {
                tracing::warn!(%error, collection = col_id, "tmdb collection fetch failed");
            }
        }
    }

    Ok(details)
}

pub(crate) async fn fetch_person(
    api: &Api,
    id: TmdbId,
    language: Option<&str>,
    net: &NetConfig,
) -> Result<PersonDetails, Error> {
    let url = api.url(&format!("person/{}", id.get()));
    let body: PersonBody = send_json(net, |client| {
        let builder = api.get(client, &url).timeout(DETAILS_TIMEOUT);
        let mut request = builder.query(&[("append_to_response", "combined_credits")]);

        if let Some(language) = language.filter(|s| !s.is_empty()) {
            request = request.query(&[("language", language)]);
        }

        request
    })
    .await?;

    person_from_body(body)
}

async fn fetch_collection(
    net: &NetConfig,
    api: &Api,
    id: u32,
    language: Option<&str>,
) -> Result<Vec<CatalogItem>, Error> {
    let url = api.url(&format!("collection/{id}"));
    let body: CollectionBody = send_json(net, |client| {
        let mut request = api.get(client, &url).timeout(DETAILS_TIMEOUT);

        if let Some(language) = language.filter(|s| !s.is_empty()) {
            request = request.query(&[("language", language)]);
        }

        request
    })
    .await?;

    Ok(catalog_items_from(body.parts.unwrap_or_default(), Some(MediaKind::Movie), MAX_ROW))
}
