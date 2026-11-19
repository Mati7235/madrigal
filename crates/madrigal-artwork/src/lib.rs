mod disk;

use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clru::CLruCache;
use image::imageops::FilterType;
use image::{ImageReader, RgbaImage};

pub use disk::{dir_size, file_stem, prune as prune_artwork};

pub const DEFAULT_BUDGET_BYTES: usize = 256 * 1024 * 1024;
pub const DEFAULT_MEMORY_BUDGET_BYTES: usize = 24 * 1024 * 1024;
pub const BUCKETS: [u32; 4] = [96, 128, 384, 640];

const FETCH_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_FETCH_BYTES: usize = 4 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;

#[derive(Clone, Debug)]
pub enum Source {
    Url(String),
    File(PathBuf),
    Bytes(Vec<u8>),
}

#[derive(Debug)]
pub enum ArtworkError {
    Io(io::Error),
    Http(String),
    Decode(String),
    InvalidBucket(u32),
    TooLarge(usize),
}

impl fmt::Display for ArtworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "artwork io: {e}"),
            Self::Http(e) => write!(f, "artwork http: {e}"),
            Self::Decode(e) => write!(f, "artwork decode: {e}"),
            Self::InvalidBucket(b) => write!(f, "invalid artwork bucket: {b}"),
            Self::TooLarge(n) => write!(f, "artwork payload too large: {n}"),
        }
    }
}

impl std::error::Error for ArtworkError {}

impl From<io::Error> for ArtworkError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

type CacheKey = (String, u32);

struct MemoryCache {
    lru: CLruCache<CacheKey, Arc<RgbaImage>>,
    bytes: usize,
    budget: usize,
}

impl MemoryCache {
    fn new(budget: usize) -> Self {
        let slots = (budget / (96 * 96 * 4)).clamp(16, 8192);
        Self {
            lru: CLruCache::new(NonZeroUsize::new(slots).unwrap_or(NonZeroUsize::MIN)),
            bytes: 0,
            budget,
        }
    }

    fn get(&mut self, key: &CacheKey) -> Option<Arc<RgbaImage>> {
        self.lru.get(key).cloned()
    }

    fn insert(&mut self, cache_key: CacheKey, image: RgbaImage) {
        let size = image.as_raw().len();
        if size == 0 || size >= self.budget {
            return;
        }
        while self.bytes + size > self.budget && !self.lru.is_empty() {
            if let Some((_, old)) = self.lru.pop_back() {
                self.bytes = self.bytes.saturating_sub(old.as_raw().len());
            }
        }
        if self.bytes + size > self.budget {
            return;
        }
        if let Some(old) = self.lru.put(cache_key, Arc::new(image)) {
            self.bytes = self.bytes.saturating_sub(old.as_raw().len());
        }
        self.bytes += size;
    }

    fn contains(&self, key: &CacheKey) -> bool {
        self.lru.contains(key)
    }

    fn len(&self) -> usize {
        self.lru.len()
    }

    fn bytes(&self) -> usize {
        self.bytes
    }
}

pub struct ArtworkCache {
    disk: disk::DiskCache,
    budget_bytes: u64,
    memory: Mutex<MemoryCache>,
    client: reqwest::blocking::Client,
}

impl ArtworkCache {
    pub fn new(dir: PathBuf, budget_bytes: usize) -> Self {
        Self::with_memory_budget(dir, budget_bytes, DEFAULT_MEMORY_BUDGET_BYTES)
    }

    pub fn with_memory_budget(dir: PathBuf, budget_bytes: usize, memory_budget: usize) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            disk: disk::DiskCache::new(&dir),
            budget_bytes: budget_bytes as u64,
            memory: Mutex::new(MemoryCache::new(memory_budget)),
            client,
        }
    }

    pub fn default_dir() -> PathBuf {
        madrigal_paths::cache_dir()
    }

    pub fn dir(&self) -> &Path {
        self.disk.dir()
    }

    pub fn get(&self, key: &str, source: Source, bucket: u32) -> Result<RgbaImage, ArtworkError> {
        if !BUCKETS.contains(&bucket) {
            return Err(ArtworkError::InvalidBucket(bucket));
        }
        let cache_key = (key.to_string(), bucket);
        if let Some(image) = self.memory.lock().map_err(|_| ArtworkError::Decode("lock poisoned".into()))?.get(&cache_key) {
            return Ok((*image).clone());
        }
        let expected = bucket as usize * bucket as usize * 4;
        if let Some(raw) = self.disk.read(key, bucket, bucket, bucket) {
            if raw.len() == expected {
                if let Some(image) = RgbaImage::from_raw(bucket, bucket, raw) {
                    self.memory
                        .lock()
                        .map_err(|_| ArtworkError::Decode("lock poisoned".into()))?
                        .insert(cache_key, image.clone());
                    return Ok(image);
                }
            }
        }
        let bytes = self.fetch(source)?;
        let image = decode(&bytes, bucket)?;
        let _ = self.disk.write(key, bucket, image.as_raw(), bucket, bucket);
        self.disk.prune(self.budget_bytes);
        self.memory
            .lock()
            .map_err(|_| ArtworkError::Decode("lock poisoned".into()))?
            .insert(cache_key, image.clone());
        Ok(image)
    }

    pub fn prune_disk(&self) -> u64 {
        self.disk.prune(self.budget_bytes)
    }

    pub fn disk_bytes(&self) -> u64 {
        self.disk.total_bytes()
    }

    pub fn memory_bytes(&self) -> usize {
        self.memory
            .lock()
            .map(|cache| cache.bytes())
            .unwrap_or(0)
    }

    pub fn memory_len(&self) -> usize {
        self.memory
            .lock()
            .map(|cache| cache.len())
            .unwrap_or(0)
    }

    pub fn contains_memory(&self, key: &str, bucket: u32) -> bool {
        self.memory
            .lock()
            .map(|cache| cache.contains(&(key.to_string(), bucket)))
            .unwrap_or(false)
    }

}

fn decode(bytes: &[u8], bucket: u32) -> Result<RgbaImage, ArtworkError> {
    let dimensions = ImageReader::new(io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ArtworkError::Decode(e.to_string()))?
        .into_dimensions()
        .map_err(|e| ArtworkError::Decode(e.to_string()))?;
    if dimensions.0 > MAX_DIMENSION || dimensions.1 > MAX_DIMENSION {
        return Err(ArtworkError::Decode(format!(
            "dimensions {}x{} exceed {MAX_DIMENSION}",
            dimensions.0, dimensions.1
        )));
    }
    let image = ImageReader::new(io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ArtworkError::Decode(e.to_string()))?
        .decode()
        .map_err(|e| ArtworkError::Decode(e.to_string()))?;
    let (width, height) = (image.width(), image.height());
    let side = width.min(height);
    let cropped = if side == 0 {
        image
    } else {
        image.crop_imm((width - side) / 2, (height - side) / 2, side, side)
    };
    Ok(image::imageops::resize(
        &cropped,
        bucket,
        bucket,
        FilterType::Lanczos3,
    ))
}

impl ArtworkCache {
    fn fetch(&self, source: Source) -> Result<Vec<u8>, ArtworkError> {
        let bytes = match source {
            Source::Bytes(bytes) => bytes,
            Source::File(path) => fs::read(path)?,
            Source::Url(url) => {
                let response = self
                    .client
                    .get(&url)
                    .header("Accept", "image/*")
                    .send()
                    .map_err(|e| ArtworkError::Http(e.to_string()))?
                    .error_for_status()
                    .map_err(|e| ArtworkError::Http(e.to_string()))?;
                let mut bytes = Vec::new();
                response
                    .take(MAX_FETCH_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)?;
                bytes
            }
        };
        if bytes.len() > MAX_FETCH_BYTES {
            return Err(ArtworkError::TooLarge(bytes.len()));
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut image = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let on = (x / 8 + y / 8) % 2 == 0;
                image.put_pixel(
                    x,
                    y,
                    if on {
                        Rgba([255, 0, 0, 255])
                    } else {
                        Rgba([0, 0, 255, 255])
                    },
                );
            }
        }
        let mut bytes = Vec::new();
        image
            .write_to(&mut io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn buckets_and_cache_hits() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ArtworkCache::with_memory_budget(tmp.path().into(), 1 << 20, 1 << 20);
        let source = Source::Bytes(png(64, 64));
        let first = cache.get("track", source, 128).unwrap();
        assert_eq!(first.dimensions(), (128, 128));
        assert!(cache.contains_memory("track", 128));
        let second = cache
            .get("track", Source::Bytes(vec![1, 2, 3]), 128)
            .unwrap();
        assert_eq!(second.dimensions(), (128, 128));
        assert_eq!(first.as_raw(), second.as_raw());
        assert!(cache.get("track", Source::Bytes(png(64, 64)), 96).unwrap().dimensions() == (96, 96));
        assert!(matches!(
            cache.get("track", Source::Bytes(Vec::new()), 100),
            Err(ArtworkError::InvalidBucket(100))
        ));
    }

    #[test]
    fn rejects_oversized_payloads_and_dimensions() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ArtworkCache::new(tmp.path().into(), 1 << 20);
        assert!(matches!(
            cache.get("big", Source::Bytes(vec![0; MAX_FETCH_BYTES + 1]), 96),
            Err(ArtworkError::TooLarge(_))
        ));
        let wide = png(MAX_DIMENSION + 1, 1);
        assert!(matches!(
            cache.get("wide", Source::Bytes(wide), 96),
            Err(ArtworkError::Decode(_))
        ));
    }

    #[test]
    fn disk_hit_across_instances() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ArtworkCache::new(tmp.path().into(), 1 << 20);
        let expected = cache.get("album", Source::Bytes(png(32, 32)), 96).unwrap();
        drop(cache);
        let fresh = ArtworkCache::new(tmp.path().into(), 1 << 20);
        let hit = fresh.get("album", Source::Bytes(vec![0]), 96).unwrap();
        assert_eq!(hit.as_raw(), expected.as_raw());
        assert!(fresh.disk_bytes() > 0);
    }

    #[test]
    fn lru_eviction() {
        let tmp = tempfile::tempdir().unwrap();
        let budget = 96 * 96 * 4 + 16;
        let cache = ArtworkCache::with_memory_budget(tmp.path().into(), 1 << 20, budget);
        cache.get("a", Source::Bytes(png(64, 64)), 96).unwrap();
        assert!(cache.contains_memory("a", 96));
        cache.get("b", Source::Bytes(png(64, 64)), 96).unwrap();
        assert!(!cache.contains_memory("a", 96));
        assert!(cache.contains_memory("b", 96));
        assert!(cache.memory_bytes() <= budget);
    }

    #[test]
    fn disk_prune_and_ttl() {
        let tmp = tempfile::tempdir().unwrap();
        let budget = 12 * 1024;
        let cache = ArtworkCache::new(tmp.path().into(), budget as usize);
        for i in 0..40 {
            cache
                .get(&format!("key-{i}"), Source::Bytes(png(32, 32)), 96)
                .unwrap();
        }
        assert!(cache.disk_bytes() <= budget);
        let meta = fs::read(
            cache
                .dir()
                .join(format!("{}.meta", file_stem("key-0", 96))),
        );
        assert!(meta.is_err(), "oldest entry should be pruned");
    }

    #[test]
    fn corrupt_source_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ArtworkCache::new(tmp.path().into(), 1 << 20);
        assert!(cache.get("bad", Source::Bytes(vec![1, 2, 3]), 96).is_err());
        assert_eq!(cache.disk_bytes(), 0);
    }
}
