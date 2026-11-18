use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use madrigal_notify::{NotificationBuilder, Notifier, ZbusBackend, BUS_NAME, OBJECT_PATH};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

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

#[derive(Debug)]
struct Recorded {
    app_name: String,
    replaces_id: u32,
    summary: String,
    body: String,
    actions: Vec<String>,
    hints: HashMap<String, OwnedValue>,
    expire_timeout: i32,
}

struct StubNotifications {
    pub recorded: Mutex<Vec<Recorded>>,
    next_id: Mutex<u32>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl StubNotifications {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        let mut next_id = self.next_id.lock().expect("id");
        *next_id += 1;
        let id = *next_id;
        self.recorded.lock().expect("recorded").push(Recorded {
            app_name,
            replaces_id,
            summary,
            body,
            actions,
            hints,
            expire_timeout,
        });
        Ok(id)
    }

    async fn get_capabilities(&self) -> zbus::fdo::Result<Vec<String>> {
        Ok(vec![
            "actions".to_owned(),
            "body".to_owned(),
            "persistence".to_owned(),
        ])
    }

    #[zbus(signal)]
    async fn action_invoked(
        emitter: SignalEmitter<'_>,
        id: u32,
        action: String,
    ) -> zbus::Result<()>;
}

async fn run(address: &str) {
    let stub = StubNotifications {
        recorded: Mutex::new(Vec::new()),
        next_id: Mutex::new(41),
    };
    let service_conn = zbus::connection::Builder::address(address)
        .expect("address")
        .name(BUS_NAME)
        .expect("name")
        .serve_at(OBJECT_PATH, stub)
        .expect("serve")
        .build()
        .await
        .expect("service conn");

    let backend = ZbusBackend::connect(Some(address)).await.expect("connect");
    let notifier = Notifier::new(backend);
    let mut actions = notifier.backend().subscribe_actions().await.expect("subscribe");

    let caps = notifier.server_capabilities().await.expect("caps");
    assert_eq!(caps, vec!["actions", "body", "persistence"]);

    let builder = NotificationBuilder::new("Never Gonna Give You Up")
        .body("Rick Astley")
        .icon("/tmp/art.jpg")
        .playback_actions("Previous", "Play/Pause", "Next")
        .timeout_ms(5000);
    let id = notifier.show(&builder).await.expect("show");
    assert_eq!(id, 42);

    tokio::time::timeout(Duration::from_secs(5), async {
        while service_conn
            .object_server()
            .interface::<_, StubNotifications>(OBJECT_PATH)
            .await
            .expect("iface")
            .get()
            .await
            .recorded
            .lock()
            .expect("recorded")
            .is_empty()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("recorded call");

    {
        let iface = service_conn
            .object_server()
            .interface::<_, StubNotifications>(OBJECT_PATH)
            .await
            .expect("iface");
        let recorded = iface.get().await;
        let calls = recorded.recorded.lock().expect("recorded");
        assert_eq!(calls.len(), 1);
        let call = &calls[0];
        assert_eq!(call.app_name, "Madrigal");
        assert_eq!(call.replaces_id, 0);
        assert_eq!(call.summary, "Never Gonna Give You Up");
        assert_eq!(call.body, "Rick Astley");
        assert_eq!(
            call.actions,
            vec![
                "default",
                "",
                "previous",
                "Previous",
                "toggle",
                "Play/Pause",
                "next",
                "Next",
            ]
        );
        assert_eq!(call.expire_timeout, 5000);
        assert!(call.hints.contains_key("image-path"));
    }

    let iface = service_conn
        .object_server()
        .interface::<_, StubNotifications>(OBJECT_PATH)
        .await
        .expect("iface");
    StubNotificationsSignals::action_invoked(&iface, id, "toggle".to_owned())
        .await
        .expect("emit action");

    let (got_id, key) = tokio::time::timeout(Duration::from_secs(5), actions.next())
        .await
        .expect("action timeout")
        .expect("action");
    assert_eq!(got_id, id);
    assert_eq!(key, "toggle");

    let mut handled = Vec::new();
    match key.as_str() {
        "previous" => handled.push("previous"),
        "toggle" => handled.push("play_pause"),
        "next" => handled.push("next"),
        _ => {}
    }
    assert_eq!(handled, vec!["play_pause"]);
}

#[test]
fn notify_actions() {
    let Some(bus) = PrivateBus::spawn() else {
        println!("SKIP notify_actions native");
        return;
    };
    let root = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    zbus::block_on(run(&bus.address));
}
