use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, DistinguishedName, Error as TlsError,
    SignatureScheme, StreamOwned,
};

use crate::media::{CastMedia, load};
use crate::protocol::{
    AppSession, CastMessage, ReceiverStatus, close, connect, decode_proto,
    encode_frame, get_status, is_ping, launch, media_stop, parse_media_session_ids,
    parse_receiver_status, pong, set_volume,
};
use crate::{Error, Result};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_AWAIT_MESSAGES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Idle,
    Connected,
    Launched,
    Loaded,
    Stopped,
    Closed,
}

pub trait Transport {
    fn send(&mut self, message: &CastMessage) -> Result<()>;
    fn recv(&mut self) -> Result<CastMessage>;
}

#[derive(Debug)]
struct AcceptDeviceCert;

impl ServerCertVerifier for AcceptDeviceCert {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }

    fn root_hint_subjects(&self) -> Option<&[DistinguishedName]> {
        None
    }
}

pub struct TlsTransport {
    stream: StreamOwned<ClientConnection, TcpStream>,
    buffer: Vec<u8>,
}

impl TlsTransport {
    pub fn connect(host: &str, port: u16) -> Result<Self> {
        let address = format!("{host}:{port}");
        let socket = TcpStream::connect_timeout(
            &address
                .parse()
                .map_err(|_| Error::Tls(format!("unresolved address {address}")))?,
            CONNECT_TIMEOUT,
        )
        .map_err(|error| Error::Tls(error.to_string()))?;
        socket
            .set_read_timeout(Some(CONNECT_TIMEOUT))
            .map_err(|error| Error::Tls(error.to_string()))?;
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|error| Error::Tls(error.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptDeviceCert))
            .with_no_client_auth();
        let name = server_name(host)?;
        let connection = ClientConnection::new(Arc::new(config), name)
            .map_err(|error| Error::Tls(error.to_string()))?;
        Ok(Self { stream: StreamOwned::new(connection, socket), buffer: Vec::new() })
    }
}

fn server_name(host: &str) -> Result<rustls::pki_types::ServerName<'static>> {
    if let Ok(address) = host.parse::<IpAddr>() {
        return Ok(rustls::pki_types::ServerName::IpAddress(address.into()));
    }
    rustls::pki_types::ServerName::try_from(host.to_owned())
        .map_err(|_| Error::Tls("invalid host".into()))
}

impl Transport for TlsTransport {
    fn send(&mut self, message: &CastMessage) -> Result<()> {
        let frame = encode_frame(message);
        self.stream
            .write_all(&frame)
            .map_err(|error| Error::Transport(error.to_string()))?;
        self.stream.flush().map_err(|error| Error::Transport(error.to_string()))?;
        Ok(())
    }

    fn recv(&mut self) -> Result<CastMessage> {
        loop {
            if self.buffer.len() >= 4 {
                let length = u32::from_be_bytes([
                    self.buffer[0],
                    self.buffer[1],
                    self.buffer[2],
                    self.buffer[3],
                ]) as usize;
                if self.buffer.len() >= 4 + length {
                    let message = decode_proto(&self.buffer[4..4 + length])?;
                    self.buffer.drain(..4 + length);
                    return Ok(message);
                }
            }
            let mut chunk = [0u8; 4096];
            let read = self
                .stream
                .read(&mut chunk)
                .map_err(|error| Error::Transport(error.to_string()))?;
            if read == 0 {
                return Err(Error::Transport("connection closed".into()));
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

pub struct Session<T: Transport> {
    transport: T,
    state: State,
    request_id: u32,
    app: Option<AppSession>,
    media_session_id: Option<i64>,
}

impl<T: Transport> Session<T> {
    pub fn new(transport: T) -> Self {
        Self { transport, state: State::Idle, request_id: 0, app: None, media_session_id: None }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn app(&self) -> Option<&AppSession> {
        self.app.as_ref()
    }

    pub fn media_session_id(&self) -> Option<i64> {
        self.media_session_id
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    fn next_request_id(&mut self) -> u32 {
        self.request_id = self.request_id.wrapping_add(1).max(1);
        self.request_id
    }

    fn require(&self, states: &[State], action: &str) -> Result<()> {
        if states.contains(&self.state) {
            return Ok(());
        }
        Err(Error::WrongState(action.into()))
    }

    pub fn connect(&mut self) -> Result<()> {
        self.require(&[State::Idle], "connect")?;
        self.transport.send(&connect()?)?;
        self.state = State::Connected;
        Ok(())
    }

    pub fn refresh_status(&mut self) -> Result<ReceiverStatus> {
        self.require(&[State::Connected, State::Launched, State::Loaded, State::Stopped], "status")?;
        let request_id = self.next_request_id();
        self.transport.send(&get_status(request_id)?)?;
        self.await_receiver_status()
    }

    pub fn launch(&mut self, app_id: &str) -> Result<()> {
        self.require(&[State::Connected, State::Launched, State::Stopped], "launch")?;
        let request_id = self.next_request_id();
        self.transport.send(&launch(app_id, request_id)?)?;
        let status = self.await_receiver_status()?;
        let app = status
            .applications
            .iter()
            .find(|app| app.app_id == app_id)
            .or_else(|| status.applications.first())
            .cloned()
            .ok_or_else(|| Error::Malformed("no application".into()))?;
        self.app = Some(app);
        self.media_session_id = None;
        self.state = State::Launched;
        Ok(())
    }

    pub fn load(&mut self, media: &CastMedia) -> Result<()> {
        self.require(&[State::Launched, State::Stopped], "load")?;
        let app = self.app.clone().ok_or_else(|| Error::WrongState("load".into()))?;
        let request_id = self.next_request_id();
        self.transport.send(&load(&app.transport_id, media, &app.session_id, request_id)?)?;
        let ids = self.await_media_status()?;
        self.media_session_id = ids.first().copied();
        self.state = State::Loaded;
        Ok(())
    }

    pub fn set_volume(&mut self, level: f32) -> Result<()> {
        self.require(
            &[State::Connected, State::Launched, State::Loaded, State::Stopped],
            "volume",
        )?;
        let request_id = self.next_request_id();
        self.transport.send(&set_volume(level, request_id)?)?;
        Ok(())
    }

    pub fn stop_media(&mut self) -> Result<()> {
        self.require(&[State::Loaded], "stop")?;
        let app = self.app.clone().ok_or_else(|| Error::WrongState("stop".into()))?;
        let session = self.media_session_id.ok_or_else(|| Error::WrongState("stop".into()))?;
        let request_id = self.next_request_id();
        self.transport.send(&media_stop(&app.transport_id, session, request_id)?)?;
        self.media_session_id = None;
        self.state = State::Stopped;
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<()> {
        self.require(
            &[State::Connected, State::Launched, State::Loaded, State::Stopped],
            "disconnect",
        )?;
        let dest = self.app.as_ref().map(|app| app.transport_id.clone()).unwrap_or_default();
        self.transport.send(&close(if dest.is_empty() {
            crate::protocol::RECEIVER_ID
        } else {
            dest.as_str()
        })?)?;
        self.app = None;
        self.media_session_id = None;
        self.state = State::Closed;
        Ok(())
    }

    fn await_receiver_status(&mut self) -> Result<ReceiverStatus> {
        for _ in 0..MAX_AWAIT_MESSAGES {
            let message = self.transport.recv()?;
            if is_ping(&message) {
                self.transport.send(&pong()?)?;
                continue;
            }
            if message.namespace == crate::protocol::NS_RECEIVER {
                if let Ok(status) = parse_receiver_status(&message.payload) {
                    return Ok(status);
                }
            }
        }
        Err(Error::Transport("no receiver status".into()))
    }

    fn await_media_status(&mut self) -> Result<Vec<i64>> {
        let mut last: Option<Vec<i64>> = None;
        for _ in 0..MAX_AWAIT_MESSAGES {
            let message = self.transport.recv()?;
            if is_ping(&message) {
                self.transport.send(&pong()?)?;
                continue;
            }
            if message.namespace == crate::protocol::NS_MEDIA {
                crate::media::check_load_failed(&message.payload)?;
                if let Ok(ids) = parse_media_session_ids(&message.payload) {
                    last = Some(ids);
                    break;
                }
            }
        }
        last.ok_or_else(|| Error::Transport("no media status".into()))
    }
}
