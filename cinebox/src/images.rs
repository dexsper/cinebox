//! Poster and extra-image download queues.

use std::collections::HashSet;

use cinebox_core::{
    CatalogItem, HomeCatalog, MediaDetails, PersonDetails, Settings, tmdb_image_url,
};
use iced::Task;
use iced::widget::image::Handle as ImageHandle;
use tracing::warn;

use crate::message::Message;
use crate::ui::home::{ExtraImages, PosterMap};

enum ImageJob {
    Plain(String),
    SoftenedBackdrop(String),
}

impl ImageJob {
    fn url(&self) -> &str {
        match self {
            Self::Plain(url) | Self::SoftenedBackdrop(url) => url,
        }
    }

    fn soften(&self) -> bool {
        matches!(self, Self::SoftenedBackdrop(_))
    }

    fn into_url(self) -> String {
        match self {
            Self::Plain(url) | Self::SoftenedBackdrop(url) => url,
        }
    }
}

pub(crate) fn queue_posters(
    posters: &PosterMap,
    settings: &Settings,
    catalog: &HomeCatalog,
) -> Task<Message> {
    queue_items(
        posters,
        settings,
        catalog.rows.iter().flat_map(|row| row.items.iter()),
    )
}

pub(crate) fn queue_media_assets(
    posters: &PosterMap,
    images: &ExtraImages,
    settings: &Settings,
    details: &MediaDetails,
) -> Task<Message> {
    let item = CatalogItem {
        id: details.id,
        kind: details.kind,
        title: String::new(),
        year: details.year,
        vote: details.vote,
        poster_path: details.poster_path.clone(),
    };

    let posters_task = queue_items(
        posters,
        settings,
        std::iter::once(&item)
            .chain(details.collection.iter())
            .chain(details.recommendations.iter())
            .chain(details.similar.iter()),
    );

    let size = settings.tmdb.poster_size.tmdb_path();
    let mut extras = Vec::new();
    if let Some(url) = tmdb_image_url(details.poster_path.as_deref(), size) {
        extras.push(ImageJob::Plain(url));
    }

    if let Some(url) = tmdb_image_url(details.backdrop_path.as_deref(), "w1280") {
        extras.push(ImageJob::SoftenedBackdrop(url));
    }

    for person in details.directors.iter().chain(details.cast.iter()) {
        if let Some(url) = tmdb_image_url(person.profile_path.as_deref(), "w185") {
            extras.push(ImageJob::Plain(url));
        }
    }

    Task::batch([posters_task, queue_urls(images, settings, extras)])
}

pub(crate) fn queue_person_assets(
    posters: &PosterMap,
    images: &ExtraImages,
    settings: &Settings,
    details: &PersonDetails,
) -> Task<Message> {
    let posters_task = queue_items(posters, settings, details.credits.iter());
    let mut urls = Vec::new();

    if let Some(url) = tmdb_image_url(details.profile_path.as_deref(), "w185") {
        urls.push(ImageJob::Plain(url));
    }

    Task::batch([posters_task, queue_urls(images, settings, urls)])
}

fn queue_items<'a>(
    posters: &PosterMap,
    settings: &Settings,
    items: impl IntoIterator<Item = &'a CatalogItem>,
) -> Task<Message> {
    let size = settings.tmdb.poster_size;
    let use_system_proxy = settings.interface.use_system_proxy;

    let tasks: Vec<_> = items
        .into_iter()
        .filter_map(|item| {
            if posters.contains_key(&(item.kind, item.id)) {
                return None;
            }
            let url = item.poster_url(size)?;
            let key = (item.kind, item.id);
            Some(Task::perform(
                async move {
                    cinebox_tmdb::download_image(&url, use_system_proxy)
                        .await
                        .map_err(|error| error.to_string())
                },
                move |result| Message::PosterLoaded { key, result },
            ))
        })
        .collect();

    Task::batch(tasks)
}

fn queue_urls(
    images: &ExtraImages,
    settings: &Settings,
    jobs: impl IntoIterator<Item = ImageJob>,
) -> Task<Message> {
    let use_system_proxy = settings.interface.use_system_proxy;
    let mut seen = HashSet::new();
    let mut tasks = Vec::new();

    for job in jobs {
        let url = job.url();
        if url.is_empty() || images.contains_key(url) || !seen.insert(url.to_owned()) {
            continue;
        }
        let soften = job.soften();
        let url = job.into_url();
        let key = url.clone();
        tasks.push(Task::perform(
            async move {
                let bytes = cinebox_tmdb::download_image(&url, use_system_proxy)
                    .await
                    .map_err(|error| error.to_string())?;
                if !soften {
                    return Ok(bytes);
                }
                match crate::ui::backdrop::soften(&bytes) {
                    Ok(soft) => Ok(soft),
                    Err(error) => {
                        warn!(%error, "backdrop soften failed");
                        Ok(bytes)
                    }
                }
            },
            move |result| Message::ImageLoaded { url: key, result },
        ));
    }

    Task::batch(tasks)
}

pub(crate) fn insert_poster(
    posters: &mut PosterMap,
    key: (cinebox_core::MediaKind, cinebox_core::TmdbId),
    result: Result<Vec<u8>, String>,
) {
    if let Ok(bytes) = result {
        posters.insert(key, ImageHandle::from_bytes(bytes));
    }
}

pub(crate) fn insert_image(images: &mut ExtraImages, url: String, result: Result<Vec<u8>, String>) {
    if let Ok(bytes) = result {
        images.insert(url, ImageHandle::from_bytes(bytes));
    }
}
