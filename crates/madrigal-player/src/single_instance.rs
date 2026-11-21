use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

pub struct SingleInstance {
    // Kept alive so the socket stays bound; dropping it stops accepting connections.
    #[allow(dead_code)]
    listener: UnixListener,
    path: PathBuf,
}

pub enum InstanceRole {
    Primary(SingleInstance),
    Secondary,
}

impl SingleInstance {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn socket_path() -> PathBuf {
    madrigal_paths::runtime_dir().join("madrigal.sock")
}

pub fn acquire() -> io::Result<InstanceRole> {
    acquire_at(&socket_path())
}

pub fn acquire_at(path: &Path) -> io::Result<InstanceRole> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match UnixListener::bind(path) {
        Ok(listener) => Ok(InstanceRole::Primary(SingleInstance {
            listener,
            path: path.to_path_buf(),
        })),
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => match UnixStream::connect(path) {
            Ok(_) => Ok(InstanceRole::Secondary),
            Err(_) => {
                std::fs::remove_file(path)?;
                let listener = UnixListener::bind(path)?;
                Ok(InstanceRole::Primary(SingleInstance {
                    listener,
                    path: path.to_path_buf(),
                }))
            }
        },
        Err(err) => Err(err),
    }
}
