use std::sync::{Arc, Mutex};

use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue};
use zbus::{connection, Connection};

use crate::provider::{
    ActionResult, LoopStatus, NowPlayingProvider, DESKTOP_ENTRY, IDENTITY, MAX_RATE, MIN_RATE,
    OBJECT_PATH, PlaybackStatus, BUS_NAME,
};

fn failed(result: ActionResult) -> zbus::fdo::Result<()> {
    result.map_err(zbus::fdo::Error::Failed)
}

struct RootIface;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl RootIface {
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        IDENTITY
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        DESKTOP_ENTRY
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<&str> {
        vec!["https"]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<&str> {
        vec![
            "audio/ogg",
            "audio/mpeg",
            "audio/flac",
            "audio/mp4",
            "audio/webm",
        ]
    }

    fn raise(&self) {}
}

pub struct PlayerIface<P> {
    provider: Arc<P>,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl<P: NowPlayingProvider> PlayerIface<P> {
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.provider.playback_status().as_mpris().to_owned()
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        self.provider.loop_status().as_mpris().to_owned()
    }

    #[zbus(property)]
    fn set_loop_status(&self, value: String) -> zbus::fdo::Result<()> {
        failed(self.provider.set_loop(LoopStatus::from_mpris(&value)))
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        self.provider.rate()
    }

    #[zbus(property)]
    fn set_rate(&self, value: f64) -> zbus::fdo::Result<()> {
        failed(self.provider.set_rate(value.clamp(MIN_RATE, MAX_RATE)))
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.provider.shuffle()
    }

    #[zbus(property)]
    fn set_shuffle(&self, value: bool) -> zbus::fdo::Result<()> {
        failed(self.provider.set_shuffle(value))
    }

    #[zbus(property)]
    fn metadata(&self) -> std::collections::HashMap<String, OwnedValue> {
        match self.provider.now_playing() {
            Some(track) => crate::provider::metadata_map(&track),
            None => std::collections::HashMap::new(),
        }
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.provider.volume().clamp(0.0, 1.0)
    }

    #[zbus(property)]
    fn set_volume(&self, value: f64) -> zbus::fdo::Result<()> {
        failed(self.provider.set_volume(value.clamp(0.0, 1.0)))
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        (self.provider.position_secs() * 1_000_000.0) as i64
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        MIN_RATE
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        MAX_RATE
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.provider.can_go_next()
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.provider.can_go_previous()
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.provider.can_play()
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.provider.can_pause()
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.provider.can_seek()
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        self.provider.can_control()
    }

    fn next(&self) -> zbus::fdo::Result<()> {
        failed(self.provider.next())
    }

    fn previous(&self) -> zbus::fdo::Result<()> {
        failed(self.provider.previous())
    }

    fn pause(&self) -> zbus::fdo::Result<()> {
        failed(self.provider.pause())
    }

    fn play_pause(&self) -> zbus::fdo::Result<()> {
        failed(self.provider.play_pause())
    }

    fn stop(&self) -> zbus::fdo::Result<()> {
        failed(self.provider.stop())
    }

    fn play(&self) -> zbus::fdo::Result<()> {
        failed(self.provider.play())
    }

    fn seek(&self, offset: i64) -> zbus::fdo::Result<()> {
        failed(self.provider.seek_relative(offset as f64 / 1_000_000.0))
    }

    fn set_position(&self, _track_id: ObjectPath<'_>, position: i64) -> zbus::fdo::Result<()> {
        failed(self.provider.seek_absolute(position as f64 / 1_000_000.0))
    }

    fn open_uri(&self, uri: String) -> zbus::fdo::Result<()> {
        if uri.starts_with("https://") {
            failed(self.provider.open_uri(&uri))
        } else {
            Ok(())
        }
    }

    #[zbus(signal)]
    async fn seeked(emitter: SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
}

#[derive(Default)]
struct LastSync {
    status: Option<PlaybackStatus>,
    track_key: Option<String>,
    volume_bits: Option<u64>,
    shuffle: Option<bool>,
    loop_status: Option<LoopStatus>,
    rate_bits: Option<u64>,
}

pub struct MprisService<P> {
    conn: Connection,
    provider: Arc<P>,
    last: Mutex<LastSync>,
}

impl<P: NowPlayingProvider> MprisService<P> {
    pub async fn start(provider: Arc<P>, bus_address: Option<&str>) -> zbus::Result<Self> {
        let builder = match bus_address {
            Some(address) => connection::Builder::address(address)?,
            None => connection::Builder::session()?,
        };
        let conn = builder
            .name(BUS_NAME)?
            .serve_at(OBJECT_PATH, RootIface)?
            .serve_at(
                OBJECT_PATH,
                PlayerIface {
                    provider: Arc::clone(&provider),
                },
            )?
            .build()
            .await?;
        Ok(Self {
            conn,
            provider,
            last: Mutex::new(LastSync::default()),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub async fn emit_seeked(&self, position_secs: f64) -> zbus::Result<()> {
        let position = (position_secs * 1_000_000.0) as i64;
        let iface = self
            .conn
            .object_server()
            .interface::<_, PlayerIface<P>>(OBJECT_PATH)
            .await?;
        PlayerIfaceSignals::seeked(&iface, position).await
    }

    pub async fn sync(&self) -> zbus::Result<()> {
        let status = self.provider.playback_status();
        let track_key = self.provider.now_playing().map(|t| t.track_key());
        let volume_bits = self.provider.volume().clamp(0.0, 1.0).to_bits();
        let shuffle = self.provider.shuffle();
        let loop_status = self.provider.loop_status();
        let rate_bits = self.provider.rate().to_bits();
        let pending = {
            let mut last = self.last.lock().expect("last sync state");
            let pending = (
                last.status != Some(status),
                last.track_key != track_key,
                last.volume_bits != Some(volume_bits),
                last.shuffle != Some(shuffle),
                last.loop_status != Some(loop_status),
                last.rate_bits != Some(rate_bits),
            );
            last.status = Some(status);
            last.track_key = track_key;
            last.volume_bits = Some(volume_bits);
            last.shuffle = Some(shuffle);
            last.loop_status = Some(loop_status);
            last.rate_bits = Some(rate_bits);
            pending
        };
        let iface = self
            .conn
            .object_server()
            .interface::<_, PlayerIface<P>>(OBJECT_PATH)
            .await?;
        let emitter = iface.signal_emitter().to_owned();
        let guard = iface.get().await;
        if pending.0 {
            guard.playback_status_changed(&emitter).await?;
        }
        if pending.1 {
            guard.metadata_changed(&emitter).await?;
        }
        if pending.2 {
            guard.volume_changed(&emitter).await?;
        }
        if pending.3 {
            guard.shuffle_changed(&emitter).await?;
        }
        if pending.4 {
            guard.loop_status_changed(&emitter).await?;
        }
        if pending.5 {
            guard.rate_changed(&emitter).await?;
        }
        Ok(())
    }
}
