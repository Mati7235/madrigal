use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const DEFAULT_DISCOVERY_PORT: u16 = 42117;
pub const PROBE_LINE: &[u8] = b"madrigal-together-discover v1";
pub const BEACON_MAGIC: &str = "madrigal-together";
pub const BEACON_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Beacon {
    pub magic: String,
    pub version: u32,
    pub session_id: String,
    pub code: String,
    pub name: String,
    pub tcp_port: u16,
}

impl Beacon {
    pub fn new(
        session_id: impl Into<String>,
        code: impl Into<String>,
        name: impl Into<String>,
        tcp_port: u16,
    ) -> Self {
        Self {
            magic: BEACON_MAGIC.to_owned(),
            version: BEACON_VERSION,
            session_id: session_id.into(),
            code: code.into(),
            name: name.into(),
            tcp_port,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == BEACON_MAGIC && self.version == BEACON_VERSION && self.tcp_port != 0
    }
}

pub struct BeaconResponder {
    addr: SocketAddr,
    running: Arc<AtomicBool>,
}

impl BeaconResponder {
    pub fn spawn(beacon: Beacon) -> io::Result<Self> {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.set_read_timeout(Some(Duration::from_millis(200)))?;
        let addr = socket.local_addr()?;
        let running = Arc::new(AtomicBool::new(true));
        let flag = Arc::clone(&running);
        let payload = serde_json::to_vec(&beacon)
            .unwrap_or_else(|_| b"{}".to_vec());
        std::thread::spawn(move || {
            let mut buf = [0u8; 256];
            while flag.load(Ordering::Relaxed) {
                match socket.recv_from(&mut buf) {
                    Ok((n, peer)) => {
                        if buf[..n] == *PROBE_LINE {
                            let _ = socket.send_to(&payload, peer);
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                    Err(_) => break,
                }
            }
        });
        Ok(Self { addr, running })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Drop for BeaconResponder {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn discover(target: SocketAddr, timeout: Duration) -> Vec<Beacon> {
    let socket = match UdpSocket::bind("127.0.0.1:0") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    if socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .is_err()
    {
        return Vec::new();
    }
    if socket.send_to(PROBE_LINE, target).is_err() {
        return Vec::new();
    }
    let deadline = std::time::Instant::now() + timeout;
    let mut out = Vec::new();
    let mut buf = [0u8; 1024];
    while std::time::Instant::now() < deadline {
        match socket.recv_from(&mut buf) {
            Ok((n, _)) => {
                if let Ok(beacon) = serde_json::from_slice::<Beacon>(&buf[..n]) {
                    if beacon.is_valid()
                        && !out.iter().any(|b: &Beacon| b.session_id == beacon.session_id)
                    {
                        out.push(beacon);
                    }
                }
            }
            Err(_) => continue,
        }
    }
    out
}

pub fn discover_default(timeout: Duration) -> Vec<Beacon> {
    let mut out = Vec::new();
    for ip in ["127.0.0.1", "255.255.255.255"] {
        if let Ok(addr) = format!("{ip}:{DEFAULT_DISCOVERY_PORT}").parse::<SocketAddr>() {
            for beacon in discover(addr, timeout) {
                if !out.iter().any(|b: &Beacon| b.session_id == beacon.session_id) {
                    out.push(beacon);
                }
            }
        }
    }
    out
}
