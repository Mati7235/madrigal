use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use madrigal_core::TrackIdentity;
use madrigal_settings::Player as PlayerConfig;

use crate::audio::{ErrorSkipper, VolumeAction, ZeroVolumePause, MAX_AUTO_SKIPS};
use crate::engine::{EndReason, Engine, EngineEvent};
use crate::queue::Queue;
use crate::session::SessionState;
use crate::sleep::{SleepMode, SleepTimer};
use crate::source::{ResolvedStream, StreamSource};
use crate::Error;

struct ResolveRequest {
    generation: u64,
    track: TrackIdentity,
}

struct ResolveDone {
    generation: u64,
    track: TrackIdentity,
    result: Result<ResolvedStream, String>,
}

struct Resolver {
    requests: mpsc::Sender<ResolveRequest>,
    results: mpsc::Receiver<ResolveDone>,
}

impl Resolver {
    fn spawn<S: StreamSource>(source: Arc<S>) -> Result<Self, Error> {
        let (requests, inbox) = mpsc::channel::<ResolveRequest>();
        let (outbox, results) = mpsc::channel();
        thread::Builder::new()
            .name("madrigal-resolve".to_owned())
            .spawn(move || {
                for request in inbox {
                    let result = source.resolve(&request.track);
                    let done = ResolveDone {
                        generation: request.generation,
                        track: request.track,
                        result,
                    };
                    if outbox.send(done).is_err() {
                        break;
                    }
                }
            })
            .map_err(Error::Io)?;
        Ok(Self { requests, results })
    }
}

pub struct Player<S: StreamSource> {
    engine: Engine,
    source: Arc<S>,
    queue: Queue,
    sleep: Option<SleepTimer>,
    skips: ErrorSkipper,
    zero: ZeroVolumePause,
    preload_enabled: bool,
    autoskip_enabled: bool,
    pending: Option<Instant>,
    generation: u64,
    awaiting: Option<u64>,
    resolver: Option<Resolver>,
}

const LOAD_GRACE: Duration = Duration::from_secs(2);

impl<S: StreamSource> Player<S> {
    pub fn new(engine: Engine, source: S) -> Self {
        Self {
            engine,
            source: Arc::new(source),
            queue: Queue::new(),
            sleep: None,
            skips: ErrorSkipper::new(MAX_AUTO_SKIPS),
            zero: ZeroVolumePause::new(),
            preload_enabled: true,
            autoskip_enabled: true,
            pending: None,
            generation: 0,
            awaiting: None,
            resolver: None,
        }
    }

    pub fn apply_settings(&mut self, config: &PlayerConfig) -> Result<(), Error> {
        self.engine.set_volume(f64::from(config.volume))?;
        self.queue.set_repeat(config.repeat_mode);
        self.queue.set_permanent_shuffle(config.permanent_shuffle);
        self.queue.set_locked(config.queue_edit_lock);
        self.preload_enabled = config.preload_next_song;
        self.autoskip_enabled = config.auto_skip_next_on_error;
        crate::audio::apply_audio(&self.engine, config)
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn source(&self) -> &S {
        self.source.as_ref()
    }

    pub fn is_idle(&self) -> bool {
        self.awaiting.is_none() && self.engine.playlist_pos().is_ok_and(|pos| pos < 0)
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn queue_mut(&mut self) -> &mut Queue {
        &mut self.queue
    }

    pub fn sleep_remaining(&self, now: Instant) -> Option<std::time::Duration> {
        self.sleep.as_ref()?.remaining(now)
    }

    pub fn arm_sleep(&mut self, mode: SleepMode, now: Instant) {
        let mut timer = match mode {
            SleepMode::Minutes(mins) => SleepTimer::minutes(mins),
            SleepMode::EndOfSong => SleepTimer::end_of_song(),
        };
        timer.arm(now);
        self.sleep = Some(timer);
    }

    pub fn cancel_sleep(&mut self) {
        self.sleep = None;
    }

    pub fn poll_sleep(&mut self, now: Instant) -> Result<bool, Error> {
        let fired = self.sleep.as_mut().is_some_and(|timer| timer.poll(now));
        if fired {
            self.sleep = None;
            self.engine.set_paused(true)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn set_volume(&mut self, volume: i32) -> Result<(), Error> {
        let requested = volume.clamp(0, 130);
        let current = self.engine.volume().unwrap_or(0.0).round() as i32;
        match self.zero.set_volume(current, requested) {
            VolumeAction::Keep => self.engine.set_volume(f64::from(requested))?,
            VolumeAction::Pause => {
                self.engine.set_volume(0.0)?;
                self.engine.set_paused(true)?;
            }
            VolumeAction::Resume => {
                self.engine.set_volume(f64::from(requested))?;
                self.engine.set_paused(false)?;
            }
        }
        Ok(())
    }

    pub fn resume(&mut self, config: &PlayerConfig) -> Result<(), Error> {
        let current = self.engine.volume().unwrap_or(0.0).round() as i32;
        if let Some(target) = self.zero.resume_target(current, config.device_mute_recovery_volume) {
            self.engine.set_volume(f64::from(target))?;
        }
        self.engine.set_paused(false)
    }

    pub fn consecutive_skips(&self) -> u32 {
        self.skips.consecutive()
    }

    pub fn restore(&mut self, state: &SessionState) -> Result<bool, Error> {
        if !self
            .queue
            .replace(state.tracks.clone(), state.position.unwrap_or(0))
        {
            return Ok(false);
        }
        self.engine.set_paused(state.paused)?;
        Ok(true)
    }

    pub fn snapshot(&self) -> Result<SessionState, Error> {
        Ok(SessionState {
            tracks: self.queue.items().to_vec(),
            position: self.queue.current_index(),
            paused: self.engine.paused()?,
        })
    }

    pub fn play(&mut self) -> Result<(), Error> {
        if self.queue.current().is_none() {
            return Err(Error::NoTrack);
        }
        self.load_current()?;
        self.engine.set_paused(false)?;
        Ok(())
    }

    pub fn play_at(&mut self, index: usize) -> Result<(), Error> {
        if self.queue.play_at(index).is_none() {
            return Err(Error::NoTrack);
        }
        self.load_current()?;
        Ok(())
    }

    pub fn next_track(&mut self) -> Result<bool, Error> {
        if self.queue.advance().is_none() {
            return Ok(false);
        }
        self.load_current()?;
        Ok(true)
    }

    pub fn prev_track(&mut self) -> Result<bool, Error> {
        if self.queue.back().is_none() {
            return Ok(false);
        }
        self.load_current()?;
        Ok(true)
    }

    pub fn preload_next(&mut self) -> Result<bool, Error> {
        if !self.preload_enabled {
            return Ok(false);
        }
        let Some(next) = self.queue.peek_next().cloned() else {
            return Ok(false);
        };
        if self.source.resolves_remotely(&next) {
            return Ok(false);
        }
        let Ok(resolved) = self.source.resolve(&next) else {
            return Ok(false);
        };
        self.engine.append(&resolved.url)?;
        Ok(true)
    }

    pub fn handle_event(&mut self, event: &EngineEvent) -> Result<bool, Error> {
        match event {
            EngineEvent::FileLoaded => {
                self.pending = None;
                self.skips.on_success();
                Ok(false)
            }
            EngineEvent::EndFile(reason) => self.on_end_file(*reason),
            _ => Ok(false),
        }
    }

    pub fn poll_failed_load(&mut self, now: Instant) -> Result<bool, Error> {
        self.apply_resolved()?;
        let mut advanced = false;
        for event in self.engine.drain() {
            match event {
                EngineEvent::FileLoaded => {
                    self.pending = None;
                    self.skips.on_success();
                }
                EngineEvent::EndFile(reason) => {
                    advanced = self.on_end_file(reason)? || advanced;
                }
                _ => {}
            }
        }
        if advanced {
            return Ok(true);
        }
        let expired = self
            .pending
            .is_some_and(|since| now.duration_since(since) >= LOAD_GRACE);
        let dead = self.engine.playlist_pos().is_ok_and(|pos| pos < 0);
        if expired && dead {
            self.pending = None;
            return self.skip_next();
        }
        Ok(false)
    }

    fn on_end_file(&mut self, reason: EndReason) -> Result<bool, Error> {
        self.pending = None;
        match reason {
            EndReason::Eof => {
                if self.sleep.as_mut().is_some_and(|timer| timer.on_track_end()) {
                    self.sleep = None;
                    self.engine.set_paused(true)?;
                    return Ok(false);
                }
                self.next_track()
            }
            EndReason::Error => self.skip_next(),
            _ => Ok(false),
        }
    }

    fn skip_next(&mut self) -> Result<bool, Error> {
        if self.autoskip_enabled && self.skips.on_error() {
            self.next_track()
        } else {
            Ok(false)
        }
    }

    fn load_current(&mut self) -> Result<(), Error> {
        let Some(track) = self.queue.current().cloned() else {
            return Err(Error::NoTrack);
        };
        self.generation = self.generation.wrapping_add(1);
        self.awaiting = None;
        self.pending = None;
        if self.source.resolves_remotely(&track) {
            self.engine.stop()?;
            return self.request_resolve(track);
        }
        let resolved = self.source.resolve(&track).map_err(Error::Resolve)?;
        self.apply_load(&resolved)
    }

    fn request_resolve(&mut self, track: TrackIdentity) -> Result<(), Error> {
        if self.resolver.is_none() {
            self.resolver = Some(Resolver::spawn(Arc::clone(&self.source))?);
        }
        let request = ResolveRequest {
            generation: self.generation,
            track,
        };
        let sent = self
            .resolver
            .as_ref()
            .is_some_and(|resolver| resolver.requests.send(request).is_ok());
        if !sent {
            self.resolver = None;
            return Err(Error::Resolve("resolver stopped".to_owned()));
        }
        self.awaiting = Some(self.generation);
        Ok(())
    }

    fn apply_resolved(&mut self) -> Result<(), Error> {
        let mut ready: Vec<ResolveDone> = Vec::new();
        let mut disconnected = false;
        if let Some(resolver) = self.resolver.as_ref() {
            loop {
                match resolver.results.try_recv() {
                    Ok(done) => ready.push(done),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected {
            self.resolver = None;
            if self.awaiting.take().is_some() {
                return Err(Error::Resolve("resolver stopped".to_owned()));
            }
        }
        for done in ready {
            if done.generation != self.generation || self.queue.current() != Some(&done.track) {
                continue;
            }
            self.awaiting = None;
            match done.result {
                Ok(resolved) => self.apply_load(&resolved)?,
                Err(detail) => {
                    self.skip_next()?;
                    return Err(Error::Resolve(detail));
                }
            }
        }
        Ok(())
    }

    fn apply_load(&mut self, resolved: &ResolvedStream) -> Result<(), Error> {
        self.engine
            .load_stream(&resolved.url, resolved.user_agent.as_deref())?;
        if resolved.start_offset_secs > 0.0 {
            self.engine.seek(resolved.start_offset_secs)?;
        }
        self.pending = Some(Instant::now());
        Ok(())
    }
}
