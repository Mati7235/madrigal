use madrigal_settings::Player as PlayerConfig;

use crate::engine::{EndReason, Engine, EngineEvent};
use crate::queue::Queue;
use crate::session::SessionState;
use crate::source::StreamSource;
use crate::Error;

pub struct Player<S: StreamSource> {
    engine: Engine,
    source: S,
    queue: Queue,
}

impl<S: StreamSource> Player<S> {
    pub fn new(engine: Engine, source: S) -> Self {
        Self {
            engine,
            source,
            queue: Queue::new(),
        }
    }

    pub fn apply_settings(&mut self, config: &PlayerConfig) -> Result<(), Error> {
        self.engine.set_volume(f64::from(config.volume))?;
        self.engine.set_speed(f64::from(config.playback_speed))?;
        self.queue.set_repeat(config.repeat_mode);
        self.queue.set_permanent_shuffle(config.permanent_shuffle);
        self.queue.set_locked(config.queue_edit_lock);
        Ok(())
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn source(&self) -> &S {
        &self.source
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn queue_mut(&mut self) -> &mut Queue {
        &mut self.queue
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
        let Some(next) = self.queue.peek_next().cloned() else {
            return Ok(false);
        };
        let resolved = self
            .source
            .resolve(&next)
            .map_err(Error::Resolve)?;
        self.engine.append(&resolved.url)?;
        Ok(true)
    }

    pub fn handle_event(&mut self, event: &EngineEvent) -> Result<bool, Error> {
        match event {
            EngineEvent::EndFile(EndReason::Eof) => self.next_track(),
            _ => Ok(false),
        }
    }

    fn load_current(&mut self) -> Result<(), Error> {
        let Some(track) = self.queue.current().cloned() else {
            return Err(Error::NoTrack);
        };
        let resolved = self.source.resolve(&track).map_err(Error::Resolve)?;
        self.engine.load(&resolved.url)?;
        if resolved.start_offset_secs > 0.0 {
            self.engine.seek(resolved.start_offset_secs)?;
        }
        Ok(())
    }
}
