//! Poster and extra-image download cache → `egui::TextureHandle`.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};

use cinebox_core::{CatalogItem, MediaKind, TmdbId};
use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use tracing::warn;

/// What's in a poster well: texture, in-flight download, or nothing.
#[derive(Clone, Copy)]
pub enum ImageSlot<'a> {
    Ready(&'a TextureHandle),
    Loading,
    Missing,
}

impl ImageSlot<'_> {
    #[must_use]
    pub fn or(self, fallback: Self) -> Self {
        match self {
            Self::Missing => fallback,
            other => other,
        }
    }
}

pub struct ImageCache {
    textures: HashMap<String, TextureHandle>,
    pending: HashSet<String>,
    failed: HashSet<String>,
    tx: Sender<(String, Result<ColorImage, String>)>,
    rx: Receiver<(String, Result<ColorImage, String>)>,
}

impl ImageCache {
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            textures: HashMap::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
            tx,
            rx,
        }
    }

    pub fn poll(&mut self, ctx: &Context) {
        while let Ok((url, result)) = self.rx.try_recv() {
            self.pending.remove(&url);
            match result {
                Ok(image) => {
                    self.failed.remove(&url);
                    let texture = ctx.load_texture(url.clone(), image, TextureOptions::LINEAR);
                    self.textures.insert(url, texture);
                }
                Err(_) => {
                    self.failed.insert(url);
                }
            }
        }
    }

    #[must_use]
    pub fn slot(&self, url: Option<&str>) -> ImageSlot<'_> {
        let Some(url) = url.filter(|u| !u.is_empty()) else {
            return ImageSlot::Missing;
        };
        if let Some(tex) = self.textures.get(url) {
            return ImageSlot::Ready(tex);
        }
        if self.pending.contains(url) {
            return ImageSlot::Loading;
        }
        ImageSlot::Missing
    }

    #[must_use]
    pub fn get(&self, url: &str) -> Option<&TextureHandle> {
        self.textures.get(url)
    }

    #[must_use]
    pub fn poster(&self, item: &CatalogItem, size: cinebox_core::PosterSize) -> ImageSlot<'_> {
        self.slot(item.poster_url(size).as_deref())
    }

    #[must_use]
    pub fn poster_key(
        &self,
        kind: MediaKind,
        id: TmdbId,
        poster_path: Option<&str>,
        size: cinebox_core::PosterSize,
    ) -> ImageSlot<'_> {
        let _ = (kind, id);
        self.slot(cinebox_core::tmdb_image_url(poster_path, size.tmdb_path()).as_deref())
    }

    pub fn request(&mut self, url: String, soften: bool, use_system_proxy: bool) {
        if url.is_empty()
            || self.textures.contains_key(&url)
            || self.failed.contains(&url)
            || !self.pending.insert(url.clone())
        {
            return;
        }

        let tx = self.tx.clone();
        egui_async::bind::ASYNC_RUNTIME.spawn(async move {
            let result = download(url.clone(), soften, use_system_proxy).await;
            let _ = tx.send((url, result));
            if let Some(ctx) = egui_async::bind::CTX.get() {
                ctx.request_repaint();
            }
        });
    }

    pub fn request_poster(&mut self, item: &CatalogItem, size: cinebox_core::PosterSize, proxy: bool) {
        if let Some(url) = item.poster_url(size) {
            self.request(url, false, proxy);
        }
    }

    pub fn clear(&mut self) {
        self.textures.clear();
        self.pending.clear();
        self.failed.clear();
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

async fn download(url: String, soften: bool, use_system_proxy: bool) -> Result<ColorImage, String> {
    let bytes = cinebox_tmdb::download_image(&url, use_system_proxy)
        .await
        .map_err(|error| error.to_string())?;

    let bytes = if soften {
        match crate::widgets::backdrop::soften(&bytes) {
            Ok(soft) => soft,
            Err(error) => {
                warn!(%error, "backdrop soften failed");
                bytes
            }
        }
    } else {
        bytes
    };
    decode(&bytes)
}

fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|error| error.to_string())?
        .into_rgba8();

    let size = [img.width() as usize, img.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, &img))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_url_is_missing() {
        let cache = ImageCache::new();
        assert!(matches!(cache.slot(None), ImageSlot::Missing));
        assert!(matches!(cache.slot(Some("")), ImageSlot::Missing));
    }

    #[test]
    fn pending_url_is_loading() {
        let mut cache = ImageCache::new();
        cache.pending.insert(String::from("https://img/a.jpg"));
        assert!(matches!(
            cache.slot(Some("https://img/a.jpg")),
            ImageSlot::Loading
        ));
    }

    #[test]
    fn failed_url_is_missing() {
        let mut cache = ImageCache::new();
        cache.failed.insert(String::from("https://img/a.jpg"));
        assert!(matches!(
            cache.slot(Some("https://img/a.jpg")),
            ImageSlot::Missing
        ));
    }
}
