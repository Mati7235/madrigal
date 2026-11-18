use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn extension_for(mime: &str) -> &'static str {
    match mime.trim().to_lowercase().as_str() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => "bin",
    }
}

pub fn artwork_dir() -> PathBuf {
    madrigal_paths::cache_dir().join("artwork")
}

pub fn cache_size_bytes(dir: &Path) -> u64 {
    let mut total = 0u64;
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

pub fn prune_artwork(dir: &Path, limit_bytes: u64) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
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
        total += meta.len();
        let stamp = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
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
            removed += 1;
        }
    }
    removed
}

pub fn store_artwork(data: &[u8], mime: &str, limit_bytes: u64) -> io::Result<PathBuf> {
    let dir = artwork_dir();
    fs::create_dir_all(&dir)?;
    let name = format!("{:016x}.{}", fnv1a64(data), extension_for(mime));
    let path = dir.join(name);
    if !path.exists() {
        madrigal_files::atomic_write_bytes(&path, data)?;
    }
    prune_artwork(&dir, limit_bytes);
    Ok(path)
}
