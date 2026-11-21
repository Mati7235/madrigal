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
use madrigal_streams::Orchestrator;
use madrigal_ui::{download_models, DownloadItem, MainShell};
use reqwest::header::{CONTENT_RANGE, RANGE, USER_AGENT};
use reqwest::StatusCode;
use slint::{ComponentHandle, ModelRc, VecModel};

const PUMP_INTERVAL: Duration = Duration::from_millis(150);
const CHUNK_BYTES: usize = 64 * 1024;
const RANGE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRequest {
    pub url: String,
    pub user_agent: Option<String>,
}

pub trait UrlResolver: Send + Sync + 'static {
    fn resolve(&self, identity: &TrackIdentity) -> Result<MediaRequest, String>;
}

pub struct StreamResolver {
    streams: Orchestrator,
}

impl StreamResolver {
    pub fn new(streams: Orchestrator) -> Self {
        Self { streams }
    }
}

impl UrlResolver for StreamResolver {
    fn resolve(&self, identity: &TrackIdentity) -> Result<MediaRequest, String> {
        let (stream, _) = self
            .streams
            .resolve(identity, None)
            .map_err(|err| err.to_string())?;
        Ok(MediaRequest {
            url: stream.url,
            user_agent: stream.user_agent,
        })
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

    fn request(
        client: &reqwest::blocking::Client,
        target: &MediaRequest,
        start: u64,
    ) -> Result<reqwest::blocking::Response, RunnerError> {
        let end = start.saturating_add(RANGE_BYTES - 1);
        let mut request = client
            .get(&target.url)
            .header(RANGE, format!("bytes={start}-{end}"));
        if let Some(agent) = &target.user_agent {
            request = request.header(USER_AGENT, agent);
        }
        request
            .send()
            .map_err(|err| RunnerError::Failed(format!("http: {err}")))
    }
}

fn content_range_total(response: &reqwest::blocking::Response) -> Option<u64> {
    let value = response.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    let (_, total) = value.rsplit_once('/')?;
    total.parse::<u64>().ok()
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
        let target = self
            .resolver
            .resolve(&ctx.identity)
            .map_err(RunnerError::Failed)?;
        if let Some(parent) = ctx.output.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| RunnerError::Failed(format!("output dir: {err}")))?;
        }
        let mut created: Option<File> = None;
        let key = ctx.identity.to_storage_string();
        let started = Instant::now();
        let mut downloaded = 0u64;
        let mut total: Option<u64> = None;
        let mut buffer = vec![0u8; CHUNK_BYTES];
        loop {
            let mut response = Self::request(client, &target, downloaded)?;
            if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
                break;
            }
            if !response.status().is_success() {
                return Err(RunnerError::Failed(format!(
                    "http status {}",
                    response.status()
                )));
            }
            if created.is_none() {
                created = Some(
                    File::create(&ctx.output)
                        .map_err(|err| RunnerError::Failed(format!("output file: {err}")))?,
                );
            }
            let Some(file) = created.as_mut() else {
                return Err(RunnerError::Failed("output file missing".to_owned()));
            };
            let ranged = response.status() == StatusCode::PARTIAL_CONTENT;
            if ranged {
                total = total.or_else(|| content_range_total(&response));
            } else {
                total = response.content_length().or(total);
            }
            let mut received = 0u64;
            loop {
                if ctx.is_cancelled() {
                    drop(created.take());
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
                received += read as u64;
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
            if received == 0 || !ranged || received < RANGE_BYTES {
                break;
            }
            if total.is_some_and(|total| downloaded >= total) {
                break;
            }
        }
        let Some(mut file) = created else {
            return Err(RunnerError::Failed("empty stream".to_owned()));
        };
        if downloaded == 0 {
            return Err(RunnerError::Failed("empty stream".to_owned()));
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

pub fn register(shell: &MainShell, settings: &Settings, streams: Orchestrator) -> Downloads {
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
    register_with(shell, Arc::new(StreamResolver::new(streams)), config)
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
