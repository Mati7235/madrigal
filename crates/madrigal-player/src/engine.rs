use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

use libmpv2::{Format, Mpv, events::Event};

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Eof,
    Stop,
    Error,
    Redirect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    StartFile,
    EndFile(EndReason),
    FileLoaded,
    Seek,
    PlaybackRestart,
    Paused(bool),
    PlaylistPos(i64),
    Position(f64),
    WarnLog { prefix: String, text: String },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    pub vo: String,
    pub ao: String,
    pub gapless_audio: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            vo: "null".to_owned(),
            ao: "null".to_owned(),
            gapless_audio: true,
        }
    }
}

struct LogRecord {
    seq: u64,
    prefix: String,
    text: String,
}

struct Shared {
    logs: Mutex<VecDeque<LogRecord>>,
    next_seq: AtomicU64,
    stop: AtomicBool,
}

pub struct Engine {
    mpv: Mpv,
    events: mpsc::Receiver<EngineEvent>,
    shared: Arc<Shared>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Engine {
    pub fn open(config: &EngineConfig) -> Result<Self, Error> {
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("vo", config.vo.as_str())?;
            init.set_option("ao", config.ao.as_str())?;
            init.set_option(
                "gapless-audio",
                if config.gapless_audio { "yes" } else { "no" },
            )?;
            Ok(())
        })
        .map_err(|err| Error::Mpv(err.to_string()))?;
        let client = mpv
            .create_client(Some("madrigal-events"))
            .map_err(|err| Error::Mpv(err.to_string()))?;
        enable_log_messages(&client);
        client
            .observe_property("pause", Format::Flag, 1)
            .map_err(|err| Error::Mpv(err.to_string()))?;
        client
            .observe_property("playlist-pos", Format::Int64, 2)
            .map_err(|err| Error::Mpv(err.to_string()))?;
        client
            .observe_property("time-pos", Format::Double, 3)
            .map_err(|err| Error::Mpv(err.to_string()))?;
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            logs: Mutex::new(VecDeque::new()),
            next_seq: AtomicU64::new(1),
            stop: AtomicBool::new(false),
        });
        let worker = {
            let events = client;
            let tx = tx;
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("madrigal-mpv-events".to_owned())
                .spawn(move || event_loop(events, tx, &shared))
                .map_err(Error::Io)?
        };
        Ok(Self {
            mpv,
            events: rx,
            shared,
            worker: Some(worker),
        })
    }

    pub fn load(&self, url: &str) -> Result<(), Error> {
        self.mpv
            .command("loadfile", &[url, "replace"])
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn append(&self, url: &str) -> Result<(), Error> {
        self.mpv
            .command("loadfile", &[url, "append"])
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn stop(&self) -> Result<(), Error> {
        self.mpv
            .command("stop", &[])
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn seek(&self, secs: f64) -> Result<(), Error> {
        self.mpv
            .set_property("time-pos", secs)
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), Error> {
        self.mpv
            .set_property("pause", paused)
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn paused(&self) -> Result<bool, Error> {
        self.mpv
            .get_property("pause")
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn set_volume(&self, volume: f64) -> Result<(), Error> {
        self.mpv
            .set_property("volume", volume)
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn volume(&self) -> Result<f64, Error> {
        self.mpv
            .get_property("volume")
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn set_speed(&self, speed: f64) -> Result<(), Error> {
        self.mpv
            .set_property("speed", speed)
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn speed(&self) -> Result<f64, Error> {
        self.mpv
            .get_property("speed")
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn set_audio_pitch_correction(&self, enabled: bool) -> Result<(), Error> {
        self.mpv
            .set_property("audio-pitch-correction", enabled)
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn audio_pitch_correction(&self) -> Result<bool, Error> {
        self.mpv
            .get_property("audio-pitch-correction")
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn position(&self) -> Option<f64> {
        self.mpv.get_property("time-pos").ok()
    }

    pub fn prop_str(&self, name: &str) -> Option<String> {
        self.mpv.get_property(name).ok()
    }

    pub fn prop_i64(&self, name: &str) -> Option<i64> {
        self.mpv.get_property(name).ok()
    }

    pub fn prop_f64(&self, name: &str) -> Option<f64> {
        self.mpv.get_property(name).ok()
    }

    pub fn duration(&self) -> Option<f64> {
        self.mpv.get_property("duration").ok()
    }

    pub fn playlist_len(&self) -> Result<i64, Error> {
        self.mpv
            .get_property("playlist-count")
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn playlist_pos(&self) -> Result<i64, Error> {
        self.mpv
            .get_property("playlist-pos")
            .map_err(|err| Error::Mpv(err.to_string()))
    }

    pub fn apply_af(&self, spec: &str) -> Result<(), Error> {
        let since = self.shared.next_seq.load(Ordering::SeqCst);
        self.mpv
            .set_property("af", spec)
            .map_err(|err| Error::Mpv(err.to_string()))?;
        let deadline = Instant::now() + Duration::from_millis(800);
        loop {
            if let Some(failure) = self.af_failure_since(since) {
                return Err(Error::Mpv(failure));
            }
            if Instant::now() >= deadline {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    pub fn events(&self) -> &mpsc::Receiver<EngineEvent> {
        &self.events
    }

    pub fn drain(&self) -> Vec<EngineEvent> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            out.push(event);
        }
        out
    }

    fn af_failure_since(&self, since: u64) -> Option<String> {
        let logs = self.shared.logs.lock().ok()?;
        logs
            .iter()
            .rev()
            .take_while(|record| record.seq >= since)
            .find_map(|record| {
                let failed = ["failed", "Failed", "not found", "Not found"]
                    .iter()
                    .any(|marker| record.text.contains(marker));
                let relevant = matches!(
                    record.prefix.as_str(),
                    "af" | "lavfi" | "cplayer" | "ad" | "ao"
                );
                if failed && relevant {
                    Some(format!("{}: {}", record.prefix, record.text.trim()))
                } else {
                    None
                }
            })
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn enable_log_messages(client: &Mpv) {
    if let Ok(level) = CString::new("trace") {
        unsafe {
            libmpv2_sys::mpv_request_log_messages(client.ctx.as_ptr(), level.as_ptr());
        }
    }
}

fn event_loop(events: Mpv, tx: mpsc::Sender<EngineEvent>, shared: &Shared) {
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            break;
        }
        match events.wait_event(0.25) {
            Some(Ok(event)) => {
                if let Some(out) = translate(&event, shared) {
                    if tx.send(out).is_err() {
                        break;
                    }
                }
            }
            Some(Err(err)) => {
                let out = EngineEvent::WarnLog {
                    prefix: "event-loop".to_owned(),
                    text: err.to_string(),
                };
                if tx.send(out).is_err() {
                    break;
                }
            }
            None => {}
        }
    }
}

fn translate(event: &Event<'_>, shared: &Shared) -> Option<EngineEvent> {
    match event {
        Event::StartFile => Some(EngineEvent::StartFile),
        Event::EndFile(reason) => Some(EngineEvent::EndFile(map_reason(*reason))),
        Event::FileLoaded => Some(EngineEvent::FileLoaded),
        Event::Seek => Some(EngineEvent::Seek),
        Event::PlaybackRestart => Some(EngineEvent::PlaybackRestart),
        Event::PropertyChange { name, change, .. } => match (*name, change) {
            ("pause", libmpv2::events::PropertyData::Flag(paused)) => {
                Some(EngineEvent::Paused(*paused))
            }
            ("playlist-pos", libmpv2::events::PropertyData::Int64(pos)) => {
                Some(EngineEvent::PlaylistPos(*pos))
            }
            ("time-pos", libmpv2::events::PropertyData::Double(pos)) => {
                Some(EngineEvent::Position(*pos))
            }
            _ => None,
        },
        Event::LogMessage {
            prefix,
            level,
            text,
            ..
        } => {
            push_log(shared, prefix, text);
            if matches!(*level, "warn" | "error" | "fatal") {
                Some(EngineEvent::WarnLog {
                    prefix: (*prefix).to_owned(),
                    text: (*text).to_owned(),
                })
            } else {
                None
            }
        }
        Event::Shutdown => Some(EngineEvent::Shutdown),
        _ => None,
    }
}

fn push_log(shared: &Shared, prefix: &str, text: &str) {
    if let Ok(mut logs) = shared.logs.lock() {
        let seq = shared.next_seq.fetch_add(1, Ordering::SeqCst);
        logs.push_back(LogRecord {
            seq,
            prefix: prefix.to_owned(),
            text: text.to_owned(),
        });
        while logs.len() > 512 {
            logs.pop_front();
        }
    }
}

fn map_reason(reason: libmpv2::EndFileReason) -> EndReason {
    use libmpv2::mpv_end_file_reason as reasons;
    if reason == reasons::Eof {
        EndReason::Eof
    } else if reason == reasons::Stop {
        EndReason::Stop
    } else if reason == reasons::Redirect {
        EndReason::Redirect
    } else {
        EndReason::Error
    }
}
