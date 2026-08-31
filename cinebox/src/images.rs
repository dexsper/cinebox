//! Poster and extra-image download cache → `egui::TextureHandle`.
//!
//! `slot()` starts a fetch only for tiles that actually paint. GPU textures are
//! generational LRU: anything shown this frame stays, cold entries drop after
//! paint once resident bytes exceed [`TEXTURE_BUDGET`]. Reloads come from
//! SQLite, not the network.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use cinebox_core::{CatalogItem, MediaKind, Store, TmdbId, image_size_key, parse_tmdb_image_url};
use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use tracing::warn;

/// Cap on decoded GPU pixels. The on-screen working set is never evicted, even
/// if it is larger; this only bounds textures from screens you already left.
const TEXTURE_BUDGET: usize = 256 * 1024 * 1024;
const FAILED_CAP: usize = 256;

/// What's in a poster well: texture, in-flight download, or nothing.
#[derive(Clone, Copy)]
pub enum ImageSlot<'a> {
    Ready(&'a TextureHandle),
    Loading,
    Missing,
}

impl ImageSlot<'_> {
    #[must_use]
    pub fn or_else(self, fallback: impl FnOnce() -> Self) -> Self {
        match self {
            Self::Missing => fallback(),
            other => other,
        }
    }
}

struct CachedTexture {
    handle: TextureHandle,
    bytes: usize,
    last_used: Cell<u64>,
}

pub struct ImageCache {
    textures: HashMap<String, CachedTexture>,
    resident_bytes: usize,
    epoch: Cell<u64>,
    pending: RefCell<HashSet<String>>,
    failed: HashSet<String>,
    proxy: Cell<bool>,
    tx: Sender<(String, Result<ColorImage, String>)>,
    rx: Receiver<(String, Result<ColorImage, String>)>,
    db: Option<Arc<Store>>,
}

impl ImageCache {
    #[must_use]
    pub fn new() -> Self {
        Self::with_db(None)
    }

    #[must_use]
    pub fn with_db(db: Option<Arc<Store>>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            textures: HashMap::new(),
            resident_bytes: 0,
            epoch: Cell::new(0),
            pending: RefCell::new(HashSet::new()),
            failed: HashSet::new(),
            proxy: Cell::new(false),
            tx,
            rx,
            db,
        }
    }

    pub fn poll(&mut self, ctx: &Context, use_system_proxy: bool) {
        self.proxy.set(use_system_proxy);
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
                Err(_) => self.remember_failed(url),
            }
        }
    }

    /// Drop cold textures after this frame's paints, then advance the epoch.
    pub fn end_frame(&mut self) {
        self.trim();
        self.epoch.set(self.epoch.get().saturating_add(1));
    }

    #[must_use]
    pub fn slot(&self, url: Option<&str>) -> ImageSlot<'_> {
        self.lookup(url, false)
    }

    /// Same as [`Self::slot`], but decodes a softened backdrop.
    #[must_use]
    pub fn backdrop(&self, url: Option<&str>) -> ImageSlot<'_> {
        self.lookup(url, true)
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

    fn lookup(&self, url: Option<&str>, soften: bool) -> ImageSlot<'_> {
        let Some(url) = url.filter(|u| !u.is_empty()) else {
            return ImageSlot::Missing;
        };

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
        let proxy = self.proxy.get();

        if let Some(db) = &self.db {
            if let Some((size, path)) = parse_tmdb_image_url(&url) {
                let key = image_size_key(&size, soften);
                if let Ok(Some(bytes)) = db.get_image(&key, &path) {
                    let tx = self.tx.clone();
                    egui_async::bind::ASYNC_RUNTIME.spawn(async move {
                        let result = decode(&bytes);
                        let _ = tx.send((url, result));
                        request_repaint();
                    });
                    return;
                }
            }
        }

        let tx = self.tx.clone();
        let db = self.db.clone();
        egui_async::bind::ASYNC_RUNTIME.spawn(async move {
            let result = download(url.clone(), soften, proxy, db).await;
            let _ = tx.send((url, result));
            request_repaint();
        });
    }

    pub fn clear(&mut self) {
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
        self.textures.insert(
            url,
            CachedTexture {
                handle,
                bytes,
                last_used: Cell::new(epoch),
            },
        );
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
        let drop = victims(
            self.textures.iter().map(|(url, entry)| Resident {
                url,
                bytes: entry.bytes,
                last_used: entry.last_used.get(),
            }),
            epoch,
            self.resident_bytes,
            TEXTURE_BUDGET,
        );

        for url in drop {
            let Some(entry) = self.textures.remove(&url) else {
                continue;
            };

            self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
        }
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

struct Resident<'a> {
    url: &'a str,
    bytes: usize,
    last_used: u64,
}

fn victims<'a>(
    items: impl Iterator<Item = Resident<'a>>,
    epoch: u64,
    mut total: usize,
    budget: usize,
) -> Vec<String> {
    if total <= budget {
        return Vec::new();
    }

    let mut cold: Vec<Resident<'a>> = items.filter(|item| item.last_used < epoch).collect();
    cold.sort_unstable_by(|a, b| a.last_used.cmp(&b.last_used).then(b.bytes.cmp(&a.bytes)));

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

async fn download(
    url: String,
    soften: bool,
    use_system_proxy: bool,
    db: Option<Arc<Store>>,
) -> Result<ColorImage, String> {
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

    if let Some(db) = db {
        if let Some((size, path)) = parse_tmdb_image_url(&url) {
            let key = image_size_key(&size, soften);
            if let Err(error) = db.put_image(&key, &path, &bytes) {
                warn!(%error, "failed to persist tmdb image");
            }
        }
    }

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

    fn item(url: &'static str, bytes: usize, last_used: u64) -> Resident<'static> {
        Resident {
            url,
            bytes,
            last_used,
        }
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
        cache
            .pending
            .borrow_mut()
            .insert(String::from("https://img/a.jpg"));

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
        assert!(drop.is_empty());
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
        assert!(drop.is_empty());
    }

    #[test]
    fn same_generation_drops_larger_first() {
        let items = [item("small", 10, 0), item("big", 50, 0)];
        let drop = victims(items.into_iter(), 1, 60, 40);
        assert_eq!(drop, vec![String::from("big")]);
    }
}
