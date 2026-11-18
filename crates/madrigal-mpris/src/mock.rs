use std::sync::Mutex;

use madrigal_settings::Player as PlayerSettings;

use crate::provider::{
    ActionResult, LoopStatus, NowPlaying, NowPlayingProvider, PlaybackStatus, rate_from_settings,
    volume_from_settings,
};

#[derive(Debug, Default)]
pub struct MockState {
    pub track: Option<NowPlaying>,
    pub status: PlaybackStatus,
    pub position_secs: f64,
    pub loop_status: LoopStatus,
    pub shuffle: bool,
    pub volume: f64,
    pub rate: f64,
}

impl MockState {
    pub fn seed_from_player(&mut self, player: &PlayerSettings) {
        self.volume = volume_from_settings(player);
        self.rate = rate_from_settings(player);
        self.shuffle = player.permanent_shuffle;
        self.loop_status = LoopStatus::from_repeat(player.repeat_mode);
    }
}

#[derive(Default)]
pub struct MockProvider {
    pub state: Mutex<MockState>,
    pub actions: Mutex<Vec<String>>,
}

impl MockProvider {
    pub fn new(state: MockState) -> Self {
        Self {
            state: Mutex::new(state),
            actions: Mutex::new(Vec::new()),
        }
    }

    pub fn record(&self, action: impl Into<String>) {
        self.actions.lock().expect("actions").push(action.into());
    }

    pub fn actions(&self) -> Vec<String> {
        self.actions.lock().expect("actions").clone()
    }

    fn with<R>(&self, f: impl FnOnce(&MockState) -> R) -> R {
        f(&self.state.lock().expect("state"))
    }
}

impl NowPlayingProvider for MockProvider {
    fn now_playing(&self) -> Option<NowPlaying> {
        self.with(|s| s.track.clone())
    }

    fn playback_status(&self) -> PlaybackStatus {
        self.with(|s| s.status)
    }

    fn position_secs(&self) -> f64 {
        self.with(|s| s.position_secs)
    }

    fn loop_status(&self) -> LoopStatus {
        self.with(|s| s.loop_status)
    }

    fn shuffle(&self) -> bool {
        self.with(|s| s.shuffle)
    }

    fn volume(&self) -> f64 {
        self.with(|s| s.volume)
    }

    fn rate(&self) -> f64 {
        self.with(|s| s.rate)
    }

    fn play(&self) -> ActionResult {
        self.record("play");
        Ok(())
    }

    fn pause(&self) -> ActionResult {
        self.record("pause");
        Ok(())
    }

    fn play_pause(&self) -> ActionResult {
        self.record("play_pause");
        Ok(())
    }

    fn stop(&self) -> ActionResult {
        self.record("stop");
        Ok(())
    }

    fn next(&self) -> ActionResult {
        self.record("next");
        Ok(())
    }

    fn previous(&self) -> ActionResult {
        self.record("previous");
        Ok(())
    }

    fn seek_relative(&self, offset_secs: f64) -> ActionResult {
        self.record(format!("seek_relative:{offset_secs}"));
        Ok(())
    }

    fn seek_absolute(&self, position_secs: f64) -> ActionResult {
        self.record(format!("seek_absolute:{position_secs}"));
        Ok(())
    }

    fn set_loop(&self, status: LoopStatus) -> ActionResult {
        self.record(format!("set_loop:{}", status.as_mpris()));
        self.state.lock().expect("state").loop_status = status;
        Ok(())
    }

    fn set_shuffle(&self, shuffle: bool) -> ActionResult {
        self.record(format!("set_shuffle:{shuffle}"));
        self.state.lock().expect("state").shuffle = shuffle;
        Ok(())
    }

    fn set_volume(&self, volume: f64) -> ActionResult {
        self.record(format!("set_volume:{volume}"));
        self.state.lock().expect("state").volume = volume;
        Ok(())
    }

    fn set_rate(&self, rate: f64) -> ActionResult {
        self.record(format!("set_rate:{rate}"));
        self.state.lock().expect("state").rate = rate;
        Ok(())
    }

    fn open_uri(&self, uri: &str) -> ActionResult {
        self.record(format!("open_uri:{uri}"));
        Ok(())
    }
}
