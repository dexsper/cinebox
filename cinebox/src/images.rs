//! Poster and extra-image download cache → `egui::TextureHandle`.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};

use cinebox_core::{CatalogItem, MediaKind, TmdbId};
use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use tracing::warn;

pub struct ImageCache {
    textures: HashMap<String, TextureHandle>,
    pending: HashSet<String>,
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
            tx,
            rx,
        }
    }

    pub fn poll(&mut self, ctx: &Context) {
        while let Ok((url, result)) = self.rx.try_recv() {
            self.pending.remove(&url);
            if let Ok(image) = result {
                let texture = ctx.load_texture(url.clone(), image, TextureOptions::LINEAR);
                self.textures.insert(url, texture);
            }
        }
    }

    #[must_use]
    pub fn get(&self, url: &str) -> Option<&TextureHandle> {
        self.textures.get(url)
    }

    #[must_use]
    pub fn poster(&self, item: &CatalogItem, size: cinebox_core::PosterSize) -> Option<&TextureHandle> {
        item.poster_url(size)
            .as_deref()
            .and_then(|url| self.textures.get(url))
    }

    #[must_use]
    pub fn poster_key(
        &self,
        kind: MediaKind,
        id: TmdbId,
        poster_path: Option<&str>,
        size: cinebox_core::PosterSize,
    ) -> Option<&TextureHandle> {
        let _ = (kind, id);
        cinebox_core::tmdb_image_url(poster_path, size.tmdb_path())
            .as_deref()
            .and_then(|url| self.textures.get(url))
    }

    pub fn request(&mut self, url: String, soften: bool, use_system_proxy: bool) {
        if url.is_empty() || self.textures.contains_key(&url) || !self.pending.insert(url.clone()) {
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
