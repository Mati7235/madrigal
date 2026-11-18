use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use futures_lite::StreamExt as _;
use madrigal_mpris::{
    LoopStatus, MockProvider, MockState, MprisService, NowPlaying, PlaybackStatus, BUS_NAME,
    OBJECT_PATH,
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{MatchRule, MessageStream, Proxy, message::Type};

const BUS_CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <auth>ANONYMOUS</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

struct PrivateBus {
    child: Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl PrivateBus {
    fn spawn() -> Option<Self> {
        if Command::new("dbus-daemon")
            .arg("--version")
            .output()
            .is_err()
        {
            return None;
        }
        let dir = tempfile::tempdir().ok()?;
        let config = dir.path().join("session.conf");
        std::fs::write(&config, BUS_CONFIG).ok()?;
        let mut child = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .arg("--print-address=1")
            .arg("--nofork")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut line = String::new();
        BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
        let address = line.trim().to_owned();
        if address.is_empty() {
            return None;
        }
        Some(Self {
            child,
            address,
            _dir: dir,
        })
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

fn canned_provider() -> MockProvider {
    MockProvider::new(MockState {
        track: Some(NowPlaying {
            id: "dQw4w9WgXcQ".to_owned(),
            title: "Never Gonna Give You Up".to_owned(),
            artists: vec!["Rick Astley".to_owned()],
            album: Some("Whenever You Need Somebody".to_owned()),
            art_url: Some("https://img/rick.jpg".to_owned()),
            url: Some("https://stream/x".to_owned()),
            length_secs: 212.0,
        }),
        status: PlaybackStatus::Playing,
        position_secs: 30.0,
        loop_status: LoopStatus::Playlist,
        shuffle: true,
        volume: 0.7,
        rate: 1.0,
    })
}

fn owned(value: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(value).expect("owned value")
}

async fn recv_body<T>(stream: &mut MessageStream, what: &str) -> T
where
    T: serde::de::DeserializeOwned + zbus::zvariant::Type,
{
    let message = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .unwrap_or_else(|_| panic!("timeout waiting for {what}"))
        .expect("stream open")
        .expect("message");
    message.body().deserialize::<T>().expect("body")
}

async fn run(address: &str) {
    let provider = Arc::new(canned_provider());
    let service = MprisService::start(Arc::clone(&provider), Some(address))
        .await
        .expect("start service");

    let client = zbus::connection::Builder::address(address)
        .expect("address")
        .build()
        .await
        .expect("client");
    let player = zbus::proxy::Builder::<Proxy>::new(&client)
        .destination(BUS_NAME)
        .expect("dest")
        .path(OBJECT_PATH)
        .expect("path")
        .interface("org.mpris.MediaPlayer2.Player")
        .expect("iface")
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .expect("player proxy");

    let metadata: HashMap<String, OwnedValue> =
        player.get_property("Metadata").await.expect("metadata");
    assert_eq!(metadata["xesam:title"], owned(Value::from("Never Gonna Give You Up")));
    assert_eq!(
        metadata["xesam:artist"],
        owned(Value::from(vec!["Rick Astley".to_owned()]))
    );
    assert_eq!(metadata["mpris:length"], owned(Value::from(212_000_000i64)));
    assert_eq!(
        metadata["xesam:album"],
        owned(Value::from("Whenever You Need Somebody"))
    );
    assert_eq!(metadata["mpris:artUrl"], owned(Value::from("https://img/rick.jpg")));
    assert_eq!(metadata["xesam:url"], owned(Value::from("https://stream/x")));
    assert_eq!(
        metadata["mpris:trackid"],
        owned(Value::from(
            OwnedObjectPath::try_from("/org/mpris/MediaPlayer2/Track/dQw4w9WgXcQ").unwrap()
        ))
    );

    let status: String = player.get_property("PlaybackStatus").await.expect("status");
    assert_eq!(status, "Playing");
    let position: i64 = player.get_property("Position").await.expect("position");
    assert_eq!(position, 30_000_000);
    let loop_status: String = player.get_property("LoopStatus").await.expect("loop");
    assert_eq!(loop_status, "Playlist");
    let shuffle: bool = player.get_property("Shuffle").await.expect("shuffle");
    assert!(shuffle);

    player.call("Play", &()).await.map(|_: ()| ()).expect("play");
    player.call("Pause", &()).await.map(|_: ()| ()).expect("pause");
    player.call("Next", &()).await.map(|_: ()| ()).expect("next");
    player
        .call("Previous", &())
        .await
        .map(|_: ()| ())
        .expect("previous");
    let actions = provider.actions();
    for expected in ["play", "pause", "next", "previous"] {
        assert!(actions.contains(&expected.to_owned()), "{actions:?}");
    }

    player
        .call("Seek", &(5_000_000i64,))
        .await
        .map(|_: ()| ())
        .expect("seek");
    let track = OwnedObjectPath::try_from("/org/mpris/MediaPlayer2/Track/dQw4w9WgXcQ").unwrap();
    player
        .call("SetPosition", &(track, 45_000_000i64))
        .await
        .map(|_: ()| ())
        .expect("set position");
    let actions = provider.actions();
    assert!(actions.contains(&"seek_relative:5".to_owned()), "{actions:?}");
    assert!(actions.contains(&"seek_absolute:45".to_owned()), "{actions:?}");

    player
        .set_property("LoopStatus", "Track")
        .await
        .expect("set loop");
    player
        .set_property("Shuffle", false)
        .await
        .expect("set shuffle");
    let actions = provider.actions();
    assert!(actions.contains(&"set_loop:Track".to_owned()), "{actions:?}");
    assert!(actions.contains(&"set_shuffle:false".to_owned()), "{actions:?}");
    let loop_status: String = player.get_property("LoopStatus").await.expect("loop");
    assert_eq!(loop_status, "Track");
    let shuffle: bool = player.get_property("Shuffle").await.expect("shuffle");
    assert!(!shuffle);

    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("org.mpris.MediaPlayer2.Player")
        .expect("iface")
        .member("Seeked")
        .expect("member")
        .build();
    let mut seeked = MessageStream::for_match_rule(rule, &client, None)
        .await
        .expect("seeked stream");
    service.emit_seeked(42.0).await.expect("emit seeked");
    let position: i64 = recv_body(&mut seeked, "Seeked").await;
    assert_eq!(position, 42_000_000);

    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("org.freedesktop.DBus.Properties")
        .expect("iface")
        .member("PropertiesChanged")
        .expect("member")
        .build();
    let mut changed = MessageStream::for_match_rule(rule, &client, None)
        .await
        .expect("changed stream");
    provider.state.lock().expect("state").status = PlaybackStatus::Paused;
    service.sync().await.expect("sync");
    let (iface, values, _): (String, HashMap<String, OwnedValue>, Vec<String>) =
        recv_body(&mut changed, "PropertiesChanged").await;
    assert_eq!(iface, "org.mpris.MediaPlayer2.Player");
    assert_eq!(values["PlaybackStatus"], owned(Value::from("Paused")));
}

#[test]
fn mpris_service() {
    let Some(bus) = PrivateBus::spawn() else {
        println!("SKIP mpris_service native");
        return;
    };
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    zbus::block_on(run(&bus.address));
}
