use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use bytes::Bytes;
use http::{HeaderValue, StatusCode};
use moka::notification::RemovalCause;
use tokio::sync::mpsc;

use super::entry::{Encoding, Entry, FileBody, Head};
use super::key::Key;

const MAGIC: [u8; 4] = *b"CBTP";
const FORMAT_VERSION: u8 = 1;
const FIXED_HEADER_LEN: usize = MAGIC.len() + size_of::<u8>() * 2 + size_of::<u16>() * 2 + size_of::<u64>() * 3;
const TMP_DIR: &str = "tmp";

/// Holds the bytes the cache took at the last save, in decimal, so that startup can count them as space it may use.
const SIZE_HINT_FILE: &str = "size";
const FREE_SPACE_PERCENT: u64 = 80;
const DELETE_BATCH: usize = 256;

/// The on-disk tier. Files live at `root/ab/<hex>`; a lookup opens that path directly, and the index exists only to
/// account for their size and pick what to evict.
#[derive(Debug)]
pub(super) struct Disk {
    root: PathBuf,
    tmp: PathBuf,
    index: moka::sync::Cache<Key, u64>,
    // Until the startup walk ends the index misses files that are on disk, so it cannot rule a key out yet.
    is_indexed: AtomicBool,
    tmp_sequence: AtomicU64,
    inline_max_bytes: u64,
}

/// What a disk lookup found: a small entry read whole, or a large one left in its file.
#[derive(Debug)]
pub(super) enum Loaded {
    Inline(Entry),
    Large(FileBody),
}

enum Found {
    Inline(Entry),
    Large(Head, fs::File),
}

struct FixedHeader {
    status: StatusCode,
    encoding: Encoding,
    fetched_at: u64,
    expires_at: u64,
    body_len: u64,
    content_type_len: usize,
}

struct ByteReader<'a>(&'a [u8]);

impl Disk {
    /// Creates the directories, clears leftover temporary files and clamps `budget_bytes` to a share of the free space,
    /// then rebuilds the index in the background so that lookups work at once.
    pub(super) async fn open(root: PathBuf, budget_bytes: u64, inline_max_bytes: u64) -> io::Result<Arc<Self>> {
        let prepare_root = root.clone();
        let prepared = tokio::task::spawn_blocking(move || prepare(&prepare_root, budget_bytes)).await;
        let clamped_bytes = prepared.map_err(io::Error::other)??;

        let (evicted_sender, evicted) = mpsc::unbounded_channel();
        let index = build_index(clamped_bytes, evicted_sender);
        let tmp = root.join(TMP_DIR);

        let disk = Arc::new(Self {
            root,
            tmp,
            index,
            is_indexed: AtomicBool::new(false),
            tmp_sequence: AtomicU64::new(0),
            inline_max_bytes,
        });

        tokio::spawn(delete_evicted(disk.root.clone(), evicted));

        let walker = Arc::clone(&disk);
        tokio::task::spawn_blocking(move || walker.rebuild_index());

        Ok(disk)
    }

    #[must_use]
    pub(super) fn weighted_size(&self) -> u64 {
        self.index.weighted_size()
    }

    #[must_use]
    pub(super) fn entry_count(&self) -> u64 {
        self.index.entry_count()
    }

    /// `None` when there is no usable file; a damaged one is deleted on the way.
    pub(super) async fn load(self: &Arc<Self>, key: Key) -> Option<Loaded> {
        // Acquire pairs with the Release store at the end of the walk, so that `true` comes with every key it inserted.
        if self.is_indexed.load(Ordering::Acquire) && !self.index.contains_key(&key) {
            return None;
        }

        let disk = Arc::clone(self);
        let found = match tokio::task::spawn_blocking(move || disk.load_blocking(key)).await {
            Ok(found) => found?,
            Err(error) => {
                tracing::debug!(%error, "cache read task failed");
                return None;
            }
        };

        let loaded = match found {
            Found::Inline(entry) => Loaded::Inline(entry),
            Found::Large(head, file) => Loaded::Large(FileBody::new(head, tokio::fs::File::from_std(file))),
        };

        Some(loaded)
    }

    /// Records the bytes the cache holds for the budget at the next start; skipped until the startup walk has counted
    /// every file, and failures are logged.
    pub(super) async fn save_size_hint(self: &Arc<Self>) {
        let disk = Arc::clone(self);

        match tokio::task::spawn_blocking(move || disk.save_size_hint_blocking()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::debug!(%error, "cannot write the cache size hint"),
            Err(error) => tracing::debug!(%error, "cache size hint task failed"),
        }
    }

    /// Writes the entry through a temporary file and a rename; failures are logged, since the cache is optional.
    pub(super) async fn store(self: &Arc<Self>, key: Key, entry: Arc<Entry>) {
        let disk = Arc::clone(self);

        match tokio::task::spawn_blocking(move || disk.store_blocking(key, &entry)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "cannot write a cache file"),
            Err(error) => tracing::warn!(%error, "cache write task failed"),
        }
    }

    fn load_blocking(&self, key: Key) -> Option<Found> {
        let path = key.path_in(&self.root);

        match read_file(&path, self.inline_max_bytes) {
            Ok(Some((found, file_len))) => {
                self.touch(key, file_len);
                Some(found)
            }
            Ok(None) => {
                remove_quietly(&path);
                self.index.invalidate(&key);
                None
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.index.invalidate(&key);
                None
            }
            Err(error) => {
                tracing::debug!(%error, "cannot read a cache file");
                None
            }
        }
    }

    // `get` feeds the popularity estimate that eviction relies on; the insert covers files the walk has not reached.
    fn touch(&self, key: Key, file_len: u64) {
        if self.index.get(&key).is_some() {
            return;
        }

        self.index.insert(key, file_len);
    }

    fn store_blocking(&self, key: Key, entry: &Entry) -> io::Result<()> {
        let header = encode_header(entry.head())?;
        // Relaxed: the counter only has to hand out distinct numbers.
        let sequence = self.tmp_sequence.fetch_add(1, Ordering::Relaxed);
        let tmp_path = self.tmp.join(format!("{}.{sequence}", key.file_name()));

        if let Err(error) = write_file(&tmp_path, &header, entry.body()) {
            remove_quietly(&tmp_path);
            return Err(error);
        }

        if let Err(error) = fs::rename(&tmp_path, key.path_in(&self.root)) {
            remove_quietly(&tmp_path);
            return Err(error);
        }

        let file_len = (header.len() as u64).saturating_add(entry.head().body_len());
        self.index.insert(key, file_len);

        Ok(())
    }

    fn save_size_hint_blocking(&self) -> io::Result<()> {
        // Acquire pairs with the Release store at the end of the walk, as in `load`.
        if !self.is_indexed.load(Ordering::Acquire) {
            return Ok(());
        }

        self.index.run_pending_tasks();

        let size = self.index.weighted_size().to_string();
        // Relaxed: the counter only has to hand out distinct numbers.
        let sequence = self.tmp_sequence.fetch_add(1, Ordering::Relaxed);
        let tmp_path = self.tmp.join(format!("{SIZE_HINT_FILE}.{sequence}"));

        if let Err(error) = fs::write(&tmp_path, size) {
            remove_quietly(&tmp_path);
            return Err(error);
        }

        let renamed = fs::rename(&tmp_path, self.root.join(SIZE_HINT_FILE));

        if renamed.is_err() {
            remove_quietly(&tmp_path);
        }

        renamed
    }

    fn rebuild_index(&self) {
        let shards = match fs::read_dir(&self.root) {
            Ok(shards) => shards,
            Err(error) => {
                tracing::warn!(%error, "cannot list the disk cache; its size stays unaccounted until files are read");
                self.is_indexed.store(true, Ordering::Release);
                return;
            }
        };

        for shard in shards.flatten() {
            self.index_shard(&shard.path());
        }

        self.is_indexed.store(true, Ordering::Release);
        // moka updates its counts in deferred maintenance, so without this the line reports zero after a walk.
        self.index.run_pending_tasks();

        tracing::info!(entries = self.index.entry_count(), bytes = self.index.weighted_size(), "disk cache indexed");

        if let Err(error) = self.save_size_hint_blocking() {
            tracing::debug!(%error, "cannot write the cache size hint");
        }
    }

    fn index_shard(&self, shard: &Path) {
        let Ok(files) = fs::read_dir(shard) else { return };

        for file in files.flatten() {
            let name = file.file_name();
            let Some(key) = name.to_str().and_then(Key::from_file_name) else { continue };

            if key.path_in(&self.root) != file.path() {
                continue;
            }

            let Ok(metadata) = file.metadata() else { continue };

            // A write that raced the walk already put the exact size in.
            self.index.entry(key).or_insert(metadata.len());
        }
    }
}

impl FixedHeader {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let mut reader = ByteReader(bytes);

        if reader.take::<4>()? != MAGIC {
            return None;
        }

        if u8::from_le_bytes(reader.take()?) != FORMAT_VERSION {
            return None;
        }

        let status = StatusCode::from_u16(u16::from_le_bytes(reader.take()?)).ok()?;
        let encoding = Encoding::from_tag(u8::from_le_bytes(reader.take()?))?;
        let fetched_at = u64::from_le_bytes(reader.take()?);
        let expires_at = u64::from_le_bytes(reader.take()?);
        let body_len = u64::from_le_bytes(reader.take()?);
        let content_type_len = usize::from(u16::from_le_bytes(reader.take()?));

        Some(Self { status, encoding, fetched_at, expires_at, body_len, content_type_len })
    }
}

impl ByteReader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;

        Some(*head)
    }
}

fn prepare(root: &Path, budget_bytes: u64) -> io::Result<u64> {
    let tmp = root.join(TMP_DIR);
    fs::create_dir_all(&tmp)?;

    for shard in 0..=u8::MAX {
        fs::create_dir_all(root.join(format!("{shard:02x}")))?;
    }

    clear_tmp(&tmp);

    Ok(clamp_budget(root, budget_bytes, read_size_hint(root)))
}

/// 0 when there is no hint yet or it is unreadable, which only makes the budget stricter.
fn read_size_hint(root: &Path) -> u64 {
    let Ok(text) = fs::read_to_string(root.join(SIZE_HINT_FILE)) else { return 0 };

    text.trim().parse().unwrap_or(0)
}

fn clear_tmp(tmp: &Path) {
    let Ok(files) = fs::read_dir(tmp) else { return };

    for file in files.flatten() {
        remove_quietly(&file.path());
    }
}

// What the cache already holds counts as room it may use, or every restart of a full cache would shrink the budget.
fn clamp_budget(root: &Path, budget_bytes: u64, cached_bytes: u64) -> u64 {
    let available = match fs4::available_space(root) {
        Ok(available) => available,
        Err(error) => {
            tracing::warn!(%error, "cannot read the free space of the cache volume, keeping the configured disk budget");
            return budget_bytes;
        }
    };

    let usable = available.saturating_add(cached_bytes);
    let allowed = usable / 100 * FREE_SPACE_PERCENT;

    if budget_bytes <= allowed {
        return budget_bytes;
    }

    tracing::warn!(
        configured = budget_bytes,
        clamped = allowed,
        cached = cached_bytes,
        "disk cache budget is over {FREE_SPACE_PERCENT}% of the free space plus the cached bytes, clamped"
    );

    allowed
}

fn build_index(budget_bytes: u64, evicted: mpsc::UnboundedSender<Key>) -> moka::sync::Cache<Key, u64> {
    let listener = move |key: Arc<Key>, _file_len: u64, cause: RemovalCause| {
        // A replaced entry points at the file that replaced it, and an explicit removal already dealt with its file.
        if !cause.was_evicted() {
            return;
        }

        if evicted.send(*key).is_err() {
            tracing::debug!("evicted cache file left on disk: the deleter has stopped");
        }
    };

    moka::sync::Cache::builder()
        .max_capacity(budget_bytes)
        .weigher(|_key: &Key, file_len: &u64| u32::try_from(*file_len).unwrap_or(u32::MAX))
        .eviction_listener(listener)
        .build()
}

async fn delete_evicted(root: PathBuf, mut evicted: mpsc::UnboundedReceiver<Key>) {
    let root = Arc::new(root);
    let mut batch = Vec::with_capacity(DELETE_BATCH);

    while evicted.recv_many(&mut batch, DELETE_BATCH).await > 0 {
        let keys = std::mem::replace(&mut batch, Vec::with_capacity(DELETE_BATCH));
        let batch_root = Arc::clone(&root);
        let deleted = tokio::task::spawn_blocking(move || delete_files(&batch_root, &keys)).await;

        if let Err(error) = deleted {
            tracing::debug!(%error, "cache delete task failed");
        }
    }
}

fn delete_files(root: &Path, keys: &[Key]) {
    for key in keys {
        remove_quietly(&key.path_in(root));
    }
}

fn remove_quietly(path: &Path) {
    let Err(error) = fs::remove_file(path) else { return };

    if error.kind() != io::ErrorKind::NotFound {
        tracing::debug!(%error, "cannot delete a cache file");
    }
}

/// `Ok(None)` when the file is damaged: wrong magic or version, a bad field, or a length that disagrees with the header.
fn read_file(path: &Path, inline_max_bytes: u64) -> io::Result<Option<(Found, u64)>> {
    let mut file = fs::File::open(path)?;
    let file_len = file.metadata()?.len();

    if file_len < FIXED_HEADER_LEN as u64 {
        return Ok(None);
    }

    let mut fixed = [0; FIXED_HEADER_LEN];
    file.read_exact(&mut fixed)?;

    let Some(header) = FixedHeader::parse(&fixed) else { return Ok(None) };

    let header_len = FIXED_HEADER_LEN + header.content_type_len;
    let expected_len = (header_len as u64).checked_add(header.body_len);

    if expected_len != Some(file_len) {
        return Ok(None);
    }

    let mut content_type = vec![0; header.content_type_len];
    file.read_exact(&mut content_type)?;

    let Ok(content_type) = HeaderValue::from_bytes(&content_type) else { return Ok(None) };

    let head = Head::new(header.status, content_type, header.encoding, header.body_len);
    let head = head.with_times(header.fetched_at, header.expires_at);

    if header.body_len > inline_max_bytes {
        return Ok(Some((Found::Large(head, file), file_len)));
    }

    let Ok(body_len) = usize::try_from(header.body_len) else { return Ok(None) };

    let mut body = vec![0; body_len];
    file.read_exact(&mut body)?;

    let entry = Entry::from_parts(head, Bytes::from(body));

    Ok(Some((Found::Inline(entry), file_len)))
}

fn write_file(path: &Path, header: &[u8], body: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(header)?;
    file.write_all(body)
}

fn encode_header(head: &Head) -> io::Result<Vec<u8>> {
    let content_type = head.content_type().as_bytes();
    let content_type_len = u16::try_from(content_type.len()).map_err(io::Error::other)?;
    let mut header = Vec::with_capacity(FIXED_HEADER_LEN + content_type.len());

    header.extend_from_slice(&MAGIC);
    header.push(FORMAT_VERSION);
    header.extend_from_slice(&head.status().as_u16().to_le_bytes());
    header.push(head.encoding().tag());
    header.extend_from_slice(&head.fetched_at().to_le_bytes());
    header.extend_from_slice(&head.expires_at().to_le_bytes());
    header.extend_from_slice(&head.body_len().to_le_bytes());
    header.extend_from_slice(&content_type_len.to_le_bytes());
    header.extend_from_slice(content_type);

    Ok(header)
}
