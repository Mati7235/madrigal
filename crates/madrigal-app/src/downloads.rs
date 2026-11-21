use std::cell::RefCell;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use madrigal_core::{Track, TrackIdentity};
use madrigal_downloads::{
    default_downloads_dir, DownloadManager, DownloadRecord, DownloadRunner, DownloadState,
    JobOptions, ManagerConfig, ProgressEvent, RunContext, RunnerError,
};
use madrigal_settings::Settings;
use madrigal_ui::{download_models, DownloadItem, MainShell};
use slint::{ComponentHandle, ModelRc, VecModel};

const PUMP_INTERVAL: Duration = Duration::from_millis(150);
const CHUNK_BYTES: usize = 64 * 1024;

pub trait UrlResolver: Send + Sync + 'static {
    fn resolve(&self, identity: &TrackIdentity) -> Result<String, String>;
}

pub struct NoResolver;

impl UrlResolver for NoResolver {
    fn resolve(&self, _identity: &TrackIdentity) -> Result<String, String> {
        Err("stream resolver not wired".to_owned())
    }
}

pub struct HttpRunner {
    client: Result<reqwest::blocking::Client, String>,
    resolver: Arc<dyn UrlResolver>,
}

impl HttpRunner {
    pub fn new(resolver: Arc<dyn UrlResolver>) -> Self {
        let client = madrigal_net::build_stream_client(&madrigal_net::Config::default())
            .map_err(|err| err.to_string());
        Self { client, resolver }
    }
}

impl DownloadRunner for HttpRunner {
    fn run(
        &self,
        ctx: &RunContext,
        report: &dyn Fn(ProgressEvent),
    ) -> Result<PathBuf, RunnerError> {
        if ctx.is_cancelled() {
            return Err(RunnerError::Cancelled);
        }
        if ctx.is_paused() {
            return Err(RunnerError::Paused);
        }
        let client = self
            .client
            .as_ref()
            .map_err(|err| RunnerError::Failed(err.clone()))?;
        let url = self
            .resolver
            .resolve(&ctx.identity)
            .map_err(RunnerError::Failed)?;
        let mut response = client
            .get(&url)
            .send()
            .map_err(|err| RunnerError::Failed(format!("http: {err}")))?;
        if !response.status().is_success() {
            return Err(RunnerError::Failed(format!(
                "http status {}",
                response.status()
            )));
        }
        let total = response.content_length();
        if let Some(parent) = ctx.output.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| RunnerError::Failed(format!("output dir: {err}")))?;
        }
        let mut file = File::create(&ctx.output)
            .map_err(|err| RunnerError::Failed(format!("output file: {err}")))?;
        let key = ctx.identity.to_storage_string();
        let started = Instant::now();
        let mut downloaded = 0u64;
        let mut buffer = vec![0u8; CHUNK_BYTES];
        loop {
            if ctx.is_cancelled() {
                drop(file);
                let _ = std::fs::remove_file(&ctx.output);
                return Err(RunnerError::Cancelled);
            }
            if ctx.is_paused() {
                return Err(RunnerError::Paused);
            }
            let read = match response.read(&mut buffer) {
                Ok(read) => read,
                Err(err) => {
                    return Err(RunnerError::Failed(format!("read: {err}")));
                }
            };
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])
                .map_err(|err| RunnerError::Failed(format!("write: {err}")))?;
            downloaded += read as u64;
            let elapsed = started.elapsed().as_secs_f64();
            let speed = (elapsed > 0.0).then(|| downloaded as f64 / elapsed);
            let percent = total
                .filter(|total| *total > 0)
                .map(|total| downloaded as f64 / total as f64 * 100.0)
                .unwrap_or(0.0);
            report(ProgressEvent {
                key: key.clone(),
                downloaded_bytes: downloaded,
                total_bytes: total,
                speed_bps: speed,
                percent,
            });
        }
        file.flush()
            .map_err(|err| RunnerError::Failed(format!("flush: {err}")))?;
        Ok(ctx.output.clone())
    }
}

struct Inner {
    manager: RefCell<Option<DownloadManager<HttpRunner>>>,
    tracks: RefCell<Vec<Track>>,
}

pub struct Downloads {
    inner: Rc<Inner>,
    _timer: slint::Timer,
}

pub fn register(shell: &MainShell, settings: &Settings) -> Downloads {
    let dir = settings.storage.downloads_dir.trim();
    let downloads_dir = if dir.is_empty() {
        default_downloads_dir()
    } else {
        PathBuf::from(dir)
    };
    let config = ManagerConfig::with_storage(
        downloads_dir,
        madrigal_paths::db_path(),
        &settings.storage,
    );
    register_with(shell, Arc::new(NoResolver), config)
}

pub fn register_with(
    shell: &MainShell,
    resolver: Arc<dyn UrlResolver>,
    config: ManagerConfig,
) -> Downloads {
    let manager = DownloadManager::new(config, HttpRunner::new(resolver));
    let inner = Rc::new(Inner {
        manager: RefCell::new(Some(manager)),
        tracks: RefCell::new(Vec::new()),
    });

    shell.on_dl_pause({
        let inner = Rc::clone(&inner);
        let weak = shell.as_weak();
        move |id| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            with_identity(&inner, id.as_str(), |identity| {
                with_manager(&inner, |manager| manager.pause(identity));
            });
            record(&shell, "dl-pause");
        }
    });

    shell.on_dl_resume({
        let inner = Rc::clone(&inner);
        let weak = shell.as_weak();
        move |id| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            with_identity(&inner, id.as_str(), |identity| {
                with_manager(&inner, |manager| manager.resume(identity));
            });
            record(&shell, "dl-resume");
        }
    });

    shell.on_dl_cancel({
        let inner = Rc::clone(&inner);
        let weak = shell.as_weak();
        move |id| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            with_identity(&inner, id.as_str(), |identity| {
                with_manager(&inner, |manager| manager.cancel(identity));
            });
            record(&shell, "dl-cancel");
        }
    });

    shell.on_dl_retry({
        let inner = Rc::clone(&inner);
        let weak = shell.as_weak();
        move |id| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            with_identity(&inner, id.as_str(), |identity| {
                with_manager(&inner, |manager| manager.retry(identity));
            });
            record(&shell, "dl-retry");
        }
    });

    shell.on_dl_retry_all({
        let inner = Rc::clone(&inner);
        let weak = shell.as_weak();
        move || {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            let guard = inner.manager.borrow();
            if let Some(manager) = guard.as_ref() {
                for track in inner.tracks.borrow().iter() {
                    if matches!(
                        manager.state(&track.identity),
                        Some(DownloadState::Failed | DownloadState::Quarantined)
                    ) {
                        manager.retry(&track.identity);
                    }
                }
            }
            drop(guard);
            record(&shell, "dl-retry-all");
        }
    });

    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, PUMP_INTERVAL, {
        let inner = Rc::clone(&inner);
        let weak = shell.as_weak();
        move || {
            if let Some(shell) = weak.upgrade() {
                apply(&inner, &shell);
            }
        }
    });

    Downloads {
        inner,
        _timer: timer,
    }
}

impl Downloads {
    pub fn enqueue(&self, track: Track, options: JobOptions) -> bool {
        let accepted = with_manager(&self.inner, |manager| manager.enqueue(track.clone(), options))
            .unwrap_or(false);
        if accepted && !self.contains(&track.identity) {
            self.inner.tracks.borrow_mut().push(track);
        }
        accepted
    }

    pub fn state(&self, identity: &TrackIdentity) -> Option<DownloadState> {
        with_manager(&self.inner, |manager| manager.state(identity)).flatten()
    }

    pub fn attempts(&self, identity: &TrackIdentity) -> Option<u32> {
        with_manager(&self.inner, |manager| manager.attempts(identity)).flatten()
    }

    pub fn error(&self, identity: &TrackIdentity) -> Option<String> {
        with_manager(&self.inner, |manager| manager.error(identity)).flatten()
    }

    pub fn output_of(&self, identity: &TrackIdentity) -> Option<PathBuf> {
        with_manager(&self.inner, |manager| manager.output_of(identity)).flatten()
    }

    pub fn refresh(&self, shell: &MainShell) {
        apply(&self.inner, shell);
    }

    fn contains(&self, identity: &TrackIdentity) -> bool {
        self.inner
            .tracks
            .borrow()
            .iter()
            .any(|track| &track.identity == identity)
    }
}

impl Drop for Downloads {
    fn drop(&mut self) {
        let manager = self.inner.manager.borrow_mut().take();
        if let Some(manager) = manager {
            manager.shutdown();
        }
    }
}

fn with_manager<T>(
    inner: &Rc<Inner>,
    f: impl FnOnce(&DownloadManager<HttpRunner>) -> T,
) -> Option<T> {
    let guard = inner.manager.borrow();
    guard.as_ref().map(f)
}

fn with_identity(inner: &Rc<Inner>, id: &str, f: impl FnOnce(&TrackIdentity)) {
    let identity = inner
        .tracks
        .borrow()
        .iter()
        .find(|track| track.identity.id == id)
        .map(|track| track.identity.clone());
    if let Some(identity) = identity {
        f(&identity);
    }
}

fn record(shell: &MainShell, action: &str) {
    shell.set_dl_action_count(shell.get_dl_action_count() + 1);
    shell.set_dl_last_action(action.into());
}

fn items(inner: &Inner) -> Vec<DownloadItem> {
    let guard = inner.manager.borrow();
    let Some(manager) = guard.as_ref() else {
        return Vec::new();
    };
    inner
        .tracks
        .borrow()
        .iter()
        .filter_map(|track| {
            let state = manager.state(&track.identity)?;
            let (downloaded_bytes, total_bytes, speed_bps) = manager
                .progress_of(&track.identity)
                .unwrap_or((0, None, None));
            let output = manager.output_of(&track.identity).unwrap_or_default();
            Some(DownloadItem {
                record: DownloadRecord::of(track, &output, state),
                state,
                downloaded_bytes,
                total_bytes,
                speed_bps,
            })
        })
        .collect()
}

fn apply(inner: &Rc<Inner>, shell: &MainShell) {
    let (active, failed) = download_models(&items(inner));
    shell.set_dl_rows(ModelRc::new(VecModel::from(active)));
    shell.set_dl_failed(ModelRc::new(VecModel::from(failed)));
}
