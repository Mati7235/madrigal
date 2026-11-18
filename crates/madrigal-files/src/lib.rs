use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn reject_bad_path(path: &Path) -> io::Result<()> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => prefix.push(component.as_os_str()),
            Component::Normal(part) => prefix.push(part),
            Component::CurDir => continue,
            Component::ParentDir => {
                return Err(invalid(format!("destino inválido: {}", path.display())));
            }
        }
        match fs::symlink_metadata(&prefix) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(invalid(format!(
                    "destino simbólico rechazado: {}",
                    path.display()
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn atomic_write_text(path: &Path, text: &str) -> io::Result<PathBuf> {
    atomic_write_bytes(path, text.as_bytes())
}

pub fn atomic_write_bytes(path: &Path, data: &[u8]) -> io::Result<PathBuf> {
    reject_bad_path(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid(format!("destino inválido: {}", path.display())))?;
    let name = path
        .file_name()
        .ok_or_else(|| invalid(format!("destino inválido: {}", path.display())))?;
    fs::create_dir_all(parent)?;
    let tag = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(
        ".{}.{}.{tag}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(data)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map(|()| path.to_path_buf())
}
