//! Poster and extra-image download cache that hands out `egui::TextureHandle`s.
//!
//! `slot()` starts a fetch only for tiles that actually paint. GPU textures are
//! generational LRU: anything shown this frame stays, cold entries drop after
//! paint once resident bytes exceed [`TEXTURE_BUDGET`]. Reloads come from
//! SQLite, not the network.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use cinebox_core::{CatalogItem, Store, image_size_key, parse_tmdb_image_url};
use cinebox_net::NetConfig;
use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use thiserror::Error;
use tracing::warn;

const KIBIBYTE: usize = 1_024;
/// Cap on decoded GPU pixels. The on-screen working set is never evicted, even
/// if it is larger; this only bounds textures from screens you already left.
const TEXTURE_BUDGET: usize = 256 * KIBIBYTE * KIBIBYTE;
const FAILED_CAP: usize = 256;

#[derive(Debug, Error)]
enum ImageError {
    #[error("cannot download image")]
    Download(#[from] cinebox_tmdb::Error),

    #[error("cannot decode image")]
    Decode(#[from] image::ImageError),
}

type Loaded = (String, Result<ColorImage, ImageError>);

/// What's in a poster well: texture, in-flight download, or nothing.
#[derive(Clone, Copy)]
pub(crate) enum ImageSlot<'a> {
    Ready(&'a TextureHandle),
    Loading,
    Missing,
}

impl ImageSlot<'_> {
    #[must_use]
    pub(crate) fn or_else(self, fallback: impl FnOnce() -> Self) -> Self {
        match self {
            Self::Missing => fallback(),
            Self::Ready(_) | Self::Loading => self,
        }
    }
}

struct CachedTexture {
    handle: TextureHandle,
    bytes: usize,
    last_used: Cell<u64>,
}

pub(crate) struct ImageCache {
    textures: HashMap<String, CachedTexture>,
    resident_bytes: usize,
    epoch: Cell<u64>,
    pending: RefCell<HashSet<String>>,
    failed: HashSet<String>,
    net: NetConfig,
    tx: Sender<Loaded>,
    rx: Receiver<Loaded>,
    db: Option<Arc<Store>>,
}

impl ImageCache {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::with_db(None)
    }

    #[must_use]
    pub(crate) fn with_db(db: Option<Arc<Store>>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            textures: HashMap::new(),
            resident_bytes: 0,
            epoch: Cell::new(0),
            pending: RefCell::new(HashSet::new()),
            failed: HashSet::new(),
            net: NetConfig::direct(),
            tx,
            rx,
            db,
        }
    }

    pub(crate) fn poll(&mut self, ctx: &Context, net: &NetConfig) {
        if self.net != *net {
            self.net = net.clone();
        }

        let epoch = self.epoch.get();
        while let Ok((url, result)) = self.rx.try_recv() {
            self.pending.borrow_mut().remove(&url);
            match result {
                Ok(image) => {
                    self.failed.remove(&url);
                    let bytes = image_bytes(&image);
                    let texture = ctx.load_texture(url.clone(), image, TextureOptions::LINEAR);
                    self.insert_texture(url, texture, bytes, epoch);
                }
                Err(error) => {
                    warn!(error = &error as &dyn std::error::Error, url, "image failed to load");
                    self.remember_failed(url);
                }
            }
        }
    }

    /// Drop cold textures after this frame's paints, then advance the epoch.
    pub(crate) fn end_frame(&mut self) {
        self.trim();
        self.epoch.set(self.epoch.get().saturating_add(1));
    }

    #[must_use]
    pub(crate) fn slot(&self, url: Option<&str>) -> ImageSlot<'_> {
        self.lookup(url, false)
    }

    /// Same as [`Self::slot`], but decodes a softened backdrop.
    #[must_use]
    pub(crate) fn backdrop(&self, url: Option<&str>) -> ImageSlot<'_> {
        self.lookup(url, true)
    }

    #[must_use]
    pub(crate) fn poster(&self, item: &CatalogItem, size: cinebox_core::PosterSize) -> ImageSlot<'_> {
        self.slot(item.poster_url(size).as_deref())
    }

    #[must_use]
    pub(crate) fn poster_path(&self, poster_path: Option<&str>, size: cinebox_core::PosterSize) -> ImageSlot<'_> {
        self.slot(cinebox_core::tmdb_image_url(poster_path, size.tmdb_path()).as_deref())
    }

    fn lookup(&self, url: Option<&str>, soften: bool) -> ImageSlot<'_> {
        let Some(url) = url.filter(|url| !url.is_empty()) else { return ImageSlot::Missing };

        if let Some(entry) = self.textures.get(url) {
            entry.last_used.set(self.epoch.get());
            return ImageSlot::Ready(&entry.handle);
        }

        self.ensure(url, soften);

        if self.pending.borrow().contains(url) {
            return ImageSlot::Loading;
        }

        ImageSlot::Missing
    }

    fn ensure(&self, url: &str, soften: bool) {
        if self.failed.contains(url) {
            return;
        }

        if self.textures.contains_key(url) {
            return;
        }

        if !self.pending.borrow_mut().insert(url.to_owned()) {
            return;
        }

        let url = url.to_owned();
        let net = self.net.clone();
        let tx = self.tx.clone();
        let db = self.db.clone();

        egui_async::bind::ASYNC_RUNTIME.spawn(async move {
            let result = load_image(url.clone(), soften, net, db).await;

            // The cache was dropped while the image loaded, so nothing waits for it or needs a repaint.
            if tx.send((url, result)).is_err() {
                return;
            }

            request_repaint();
        });
    }

    pub(crate) fn clear(&mut self) {
        self.textures.clear();
        self.resident_bytes = 0;
        self.epoch.set(0);
        self.pending.borrow_mut().clear();
        self.failed.clear();
    }

    fn insert_texture(&mut self, url: String, handle: TextureHandle, bytes: usize, epoch: u64) {
        if let Some(old) = self.textures.remove(&url) {
            self.resident_bytes = self.resident_bytes.saturating_sub(old.bytes);
        }

        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        self.textures.insert(url, CachedTexture { handle, bytes, last_used: Cell::new(epoch) });
    }

    fn remember_failed(&mut self, url: String) {
        if self.failed.len() >= FAILED_CAP {
            self.failed.clear();
        }

        self.failed.insert(url);
    }

    fn trim(&mut self) {
        if self.resident_bytes <= TEXTURE_BUDGET {
            return;
        }

        let epoch = self.epoch.get();
        let residents = self.textures.iter().map(|(url, entry)| Resident {
            url,
            bytes: entry.bytes,
            last_used: entry.last_used.get(),
        });
        let cold = victims(residents, epoch, self.resident_bytes, TEXTURE_BUDGET);

        for url in cold {
            let Some(entry) = self.textures.remove(&url) else { continue };

            self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
        }
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ImageCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageCache")
            .field("textures", &self.textures.len())
            .field("resident_bytes", &self.resident_bytes)
            .field("epoch", &self.epoch.get())
            .field("pending", &self.pending.borrow().len())
            .field("failed", &self.failed.len())
            .field("has_db", &self.db.is_some())
            .finish_non_exhaustive()
    }
}

struct Resident<'a> {
    url: &'a str,
    bytes: usize,
    last_used: u64,
}

fn victims<'a>(items: impl Iterator<Item = Resident<'a>>, epoch: u64, mut total: usize, budget: usize) -> Vec<String> {
    if total <= budget {
        return Vec::new();
    }

    let mut cold: Vec<Resident<'a>> = items.filter(|item| item.last_used < epoch).collect();
    cold.sort_unstable_by(|left, right| left.last_used.cmp(&right.last_used).then(right.bytes.cmp(&left.bytes)));

    let mut out = Vec::new();
    for item in cold {
        if total <= budget {
            break;
        }

        total = total.saturating_sub(item.bytes);
        out.push(item.url.to_owned());
    }

    out
}

fn image_bytes(image: &ColorImage) -> usize {
    image.pixels.len().saturating_mul(4)
}

fn request_repaint() {
    if let Some(ctx) = egui_async::bind::CTX.get() {
        ctx.request_repaint();
    }
}

async fn load_image(
    url: String,
    soften: bool,
    net: NetConfig,
    db: Option<Arc<Store>>,
) -> Result<ColorImage, ImageError> {
    let cached = match db.as_deref().zip(cache_key(&url, soften)) {
        // A missing or unreadable row only means the image comes from the network again.
        Some((store, (key, path))) => store.get_image(&key, &path).await.ok().flatten(),
        None => None,
    };

    if let Some(bytes) = cached {
        return decode(&bytes);
    }

    download(url, soften, net, db).await
}

async fn download(url: String, soften: bool, net: NetConfig, db: Option<Arc<Store>>) -> Result<ColorImage, ImageError> {
    let downloaded = cinebox_tmdb::download_image(&url, &net).await?;
    let bytes = if soften { soften_or_keep(downloaded) } else { downloaded };

    let Some((store, (key, path))) = db.as_deref().zip(cache_key(&url, soften)) else { return decode(&bytes) };

    if let Err(error) = store.put_image(&key, &path, &bytes).await {
        warn!(%error, "failed to persist tmdb image");
    }

    decode(&bytes)
}

/// The size key and path a TMDB image is cached under; `None` for any other URL.
fn cache_key(url: &str, soften: bool) -> Option<(String, String)> {
    let (size, path) = parse_tmdb_image_url(url)?;

    Some((image_size_key(&size, soften), path))
}

fn soften_or_keep(bytes: Vec<u8>) -> Vec<u8> {
    match crate::widgets::backdrop::soften(&bytes) {
        Ok(softened) => softened,
        Err(error) => {
            warn!(error = &error as &dyn std::error::Error, "backdrop soften failed");
            bytes
        }
    }
}

fn decode(bytes: &[u8]) -> Result<ColorImage, ImageError> {
    let decoded = image::load_from_memory(bytes)?.into_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];

    Ok(ColorImage::from_rgba_unmultiplied(size, &decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(url: &'static str, bytes: usize, last_used: u64) -> Resident<'static> {
        Resident { url, bytes, last_used }
    }

    #[test]
    fn empty_url_is_missing() {
        let cache = ImageCache::new();
        assert!(matches!(cache.slot(None), ImageSlot::Missing));
        assert!(matches!(cache.slot(Some("")), ImageSlot::Missing));
    }

    #[test]
    fn pending_url_is_loading() {
        let cache = ImageCache::new();
        cache.pending.borrow_mut().insert(String::from("https://img/a.jpg"));

        assert!(matches!(cache.slot(Some("https://img/a.jpg")), ImageSlot::Loading));
    }

    #[test]
    fn failed_url_is_missing() {
        let mut cache = ImageCache::new();
        cache.failed.insert(String::from("https://img/a.jpg"));
        assert!(matches!(cache.slot(Some("https://img/a.jpg")), ImageSlot::Missing));
    }

    #[test]
    fn missing_falls_back_lazily() {
        let mut called = false;
        let missed = ImageSlot::Missing.or_else(|| {
            called = true;
            ImageSlot::Loading
        });
        assert!(matches!(missed, ImageSlot::Loading));
        assert!(called);

        called = false;
        let kept = ImageSlot::Loading.or_else(|| {
            called = true;
            ImageSlot::Missing
        });
        assert!(matches!(kept, ImageSlot::Loading));
        assert!(!called);
    }

    #[test]
    fn under_budget_keeps_cold() {
        let items = [item("old", 10, 0), item("hot", 10, 1)];
        let drop = victims(items.into_iter(), 1, 20, 32);
        assert_eq!(drop, [] as [String; 0]);
    }

    #[test]
    fn over_budget_drops_cold_not_hot() {
        let items = [item("hot", 80, 2), item("older", 30, 0), item("old", 20, 1)];
        let drop = victims(items.into_iter(), 2, 130, 100);
        assert_eq!(drop, vec![String::from("older")]);
    }

    #[test]
    fn hot_working_set_can_exceed_budget() {
        let items = [item("a", 80, 3), item("b", 80, 3)];
        let drop = victims(items.into_iter(), 3, 160, 100);
        assert_eq!(drop, [] as [String; 0]);
    }

    #[test]
    fn same_generation_drops_larger_first() {
        let items = [item("small", 10, 0), item("big", 50, 0)];
        let drop = victims(items.into_iter(), 1, 60, 40);
        assert_eq!(drop, vec![String::from("big")]);
    }
}
