use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

const MAGIC: &[u8; 4] = b"MAMD";
const META_LEN: usize = 20;

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn file_stem(key: &str, bucket: u32) -> String {
    format!("{:016x}-{}", fnv1a64(key.as_bytes()), bucket)
}

pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

pub fn prune(root: &Path, limit_bytes: u64) -> u64 {
    let dir = root.join("artwork");
    let Ok(entries) = fs::read_dir(&dir) else {
        return 0;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let mut stamp = meta.modified().unwrap_or(UNIX_EPOCH);
        if path.extension().and_then(|e| e.to_str()) == Some("meta") {
            let sibling = path.with_extension("rgba");
            if let Ok(sibling_meta) = fs::metadata(&sibling) {
                stamp = sibling_meta.modified().unwrap_or(stamp);
            }
        }
        total += meta.len();
        files.push((stamp, meta.len(), path));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
    let mut removed = 0;
    for (_, size, path) in files {
        if total <= limit_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
            removed += size;
        }
    }
    removed
}

pub struct DiskCache {
    dir: PathBuf,
}

impl DiskCache {
    pub fn new(root: &Path) -> Self {
        Self {
            dir: root.join("artwork"),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn rgba_path(&self, key: &str, bucket: u32) -> PathBuf {
        self.dir.join(format!("{}.rgba", file_stem(key, bucket)))
    }

    fn meta_path(&self, key: &str, bucket: u32) -> PathBuf {
        self.dir.join(format!("{}.meta", file_stem(key, bucket)))
    }

    pub fn read(&self, key: &str, bucket: u32, width: u32, height: u32) -> Option<Vec<u8>> {
        if width == 0 || height == 0 {
            return None;
        }
        let meta = fs::read(self.meta_path(key, bucket)).ok()?;
        if meta.len() != META_LEN || &meta[..4] != MAGIC {
            return None;
        }
        let stored_w = u32::from_le_bytes(meta[4..8].try_into().ok()?);
        let stored_h = u32::from_le_bytes(meta[8..12].try_into().ok()?);
        let created = u64::from_le_bytes(meta[12..20].try_into().ok()?);
        if stored_w != width || stored_h != height {
            return None;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        if now.saturating_sub(created) > TTL.as_secs() {
            return None;
        }
        let rgba = fs::read(self.rgba_path(key, bucket)).ok()?;
        if rgba.len() != width as usize * height as usize * 4 {
            return None;
        }
        Some(rgba)
    }

    pub fn write(&self, key: &str, bucket: u32, rgba: &[u8], width: u32, height: u32) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut meta = Vec::with_capacity(META_LEN);
        meta.extend_from_slice(MAGIC);
        meta.extend_from_slice(&width.to_le_bytes());
        meta.extend_from_slice(&height.to_le_bytes());
        meta.extend_from_slice(&created.to_le_bytes());
        madrigal_files::atomic_write_bytes(&self.rgba_path(key, bucket), rgba)?;
        madrigal_files::atomic_write_bytes(&self.meta_path(key, bucket), &meta)?;
        Ok(())
    }

    pub fn total_bytes(&self) -> u64 {
        dir_size(&self.dir)
    }

    pub fn prune(&self, limit_bytes: u64) -> u64 {
        prune(self.dir.parent().unwrap_or(Path::new(".")), limit_bytes)
    }
}
