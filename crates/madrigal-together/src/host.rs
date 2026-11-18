use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use madrigal_core::Track;

use crate::discovery::{Beacon, BeaconResponder};
use crate::link::JoinInfo;
use crate::models::{
    ACTIVITY_CAP, ActivityEvent, ActivityKind, Participant, PlayState, Role, RoomFlags,
    SessionCode, SessionSnapshot, new_id,
};
use crate::proto::{
    AddMode, ControlAction, Message, PROTOCOL_VERSION, decode_line, frame, server_error,
};

const READ_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub display_name: String,
    pub welcome_message: String,
    pub flags: RoomFlags,
    pub session_id: String,
    pub session_key: String,
    pub code: SessionCode,
    pub tcp_port: u16,
}

impl HostConfig {
    pub fn new(display_name: impl Into<String>) -> Self {
        Self {
            display_name: {
                let name = display_name.into();
                if name.trim().is_empty() {
                    crate::default_display_name()
                } else {
                    name
                }
            },
            welcome_message: String::new(),
            flags: RoomFlags::default(),
            session_id: new_id("session"),
            session_key: new_id("key"),
            code: SessionCode::generate(),
            tcp_port: 0,
        }
    }

    pub fn with_display_name_from_settings() -> Self {
        Self::new(crate::default_display_name())
    }
}

struct ClientRec {
    name: String,
    client_id: String,
    role: Role,
    pending: bool,
    sender: SyncSender<Out>,
    killer: TcpStream,
}

struct State {
    session_id: String,
    session_key: String,
    code: SessionCode,
    display_name: String,
    welcome_message: String,
    flags: RoomFlags,
    authority_id: String,
    clients: HashMap<String, ClientRec>,
    order: Vec<String>,
    queue: Vec<Track>,
    play: PlayState,
    repeat_mode: u8,
    shuffle: bool,
    banned_client_ids: HashSet<String>,
    activity: VecDeque<ActivityEvent>,
    seq: u64,
}

impl State {
    fn push_event(&mut self, kind: ActivityKind, actor_id: &str, detail: String) {
        self.seq += 1;
        self.activity.push_back(ActivityEvent {
            seq: self.seq,
            kind,
            actor_id: actor_id.to_owned(),
            detail,
        });
        while self.activity.len() > ACTIVITY_CAP {
            self.activity.pop_front();
        }
    }

    fn snapshot(&self) -> SessionSnapshot {
        let mut participants = vec![Participant {
            id: String::from("host"),
            name: self.display_name.clone(),
            role: if self.authority_id == "host" {
                Role::Host
            } else {
                Role::Guest
            },
            pending: false,
            connected: true,
        }];
        for (id, rec) in self.ordered_clients() {
            participants.push(Participant {
                id: id.clone(),
                name: rec.name.clone(),
                role: rec.role,
                pending: rec.pending,
                connected: true,
            });
        }
        SessionSnapshot {
            session_id: self.session_id.clone(),
            code: self.code.0.clone(),
            host_id: self.authority_id.clone(),
            participants,
            flags: self.flags,
            queue: self.queue.clone(),
            play: self.play,
            repeat_mode: self.repeat_mode,
            shuffle: self.shuffle,
            welcome_message: self.welcome_message.clone(),
        }
    }

    fn ordered_clients(&self) -> Vec<(&String, &ClientRec)> {
        let mut out = Vec::new();
        for id in &self.order {
            if let Some(rec) = self.clients.get(id) {
                out.push((id, rec));
            }
        }
        for (id, rec) in &self.clients {
            if !self.order.contains(id) {
                out.push((id, rec));
            }
        }
        out
    }

    fn writers(&self) -> Vec<(String, SyncSender<Out>)> {
        self.clients
            .iter()
            .map(|(id, rec)| (id.clone(), rec.sender.clone()))
            .collect()
    }
}

const MAILBOX: usize = 64;

pub enum Out {
    Text(String),
    Close,
}

fn spawn_writer(stream: TcpStream, rx: Receiver<Out>) {
    std::thread::spawn(move || {
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        let mut stream = stream;
        for out in rx {
            let done = match out {
                Out::Text(frame) => {
                    stream.write_all(frame.as_bytes()).is_err() || stream.flush().is_err()
                }
                Out::Close => true,
            };
            if done {
                let _ = stream.shutdown(Shutdown::Both);
                break;
            }
        }
    });
}

fn push_frame(sender: &SyncSender<Out>, text: &str) -> bool {
    push_out(sender, Out::Text(text.to_owned()))
}

fn push_out(sender: &SyncSender<Out>, out: Out) -> bool {
    match sender.try_send(out) {
        Ok(()) => true,
        Err(std::sync::mpsc::TrySendError::Full(out)) => sender.send(out).is_ok(),
        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => false,
    }
}

fn emit(shared: &Arc<Mutex<State>>, message: &Message) {
    let text = frame(message);
    let senders: Vec<SyncSender<Out>> = match shared.lock() {
        Ok(s) => s.writers().into_iter().map(|(_, sender)| sender).collect(),
        Err(_) => return,
    };
    for sender in senders {
        push_frame(&sender, &text);
    }
}

fn emit_state(shared: &Arc<Mutex<State>>) {
    let snapshot = match shared.lock() {
        Ok(s) => s.snapshot(),
        Err(_) => return,
    };
    emit(shared, &Message::RoomState { state: snapshot });
}

fn send_direct(shared: &Arc<Mutex<State>>, participant_id: &str, message: &Message) -> bool {
    let sender = match shared.lock() {
        Ok(s) => s.clients.get(participant_id).map(|r| r.sender.clone()),
        Err(_) => return false,
    };
    match sender {
        Some(sender) => push_frame(&sender, &frame(message)),
        None => false,
    }
}

fn remove_client(shared: &Arc<Mutex<State>>, participant_id: &str, reason: &str) -> bool {
    let left = {
        let mut s = match shared.lock() {
            Ok(s) => s,
            Err(_) => return false,
        };
        if s.clients.remove(participant_id).is_none() {
            return false;
        }
        s.order.retain(|x| x != participant_id);
        s.push_event(ActivityKind::Leave, participant_id, reason.to_owned());
        Message::ParticipantLeft {
            session_id: s.session_id.clone(),
            participant_id: participant_id.to_owned(),
            reason: Some(reason.to_owned()),
        }
    };
    emit(shared, &left);
    emit_state(shared);
    true
}

fn apply_control(state: &mut State, action: &ControlAction) -> Result<String, String> {
    let play = &mut state.play;
    match action {
        ControlAction::Play => {
            play.is_playing = true;
            Ok(String::from("play"))
        }
        ControlAction::Pause => {
            play.is_playing = false;
            Ok(String::from("pause"))
        }
        ControlAction::SeekTo { position_ms } => {
            play.position_ms = *position_ms;
            Ok(format!("seek {position_ms}"))
        }
        ControlAction::SkipNext => {
            if play.track_index + 1 < state.queue.len() {
                play.track_index += 1;
            }
            play.position_ms = 0;
            Ok(format!("skip_next {}", play.track_index))
        }
        ControlAction::SkipPrevious => {
            play.track_index = play.track_index.saturating_sub(1);
            play.position_ms = 0;
            Ok(format!("skip_previous {}", play.track_index))
        }
        ControlAction::SeekToIndex { index, position_ms } => {
            if *index >= state.queue.len() {
                return Err(format!("index {index} out of range"));
            }
            play.track_index = *index;
            play.position_ms = *position_ms;
            Ok(format!("seek_index {index}"))
        }
        ControlAction::SeekToTrack {
            track_id,
            position_ms,
        } => {
            let index = state
                .queue
                .iter()
                .position(|t| t.identity.to_storage_string() == *track_id)
                .ok_or_else(|| format!("unknown track {track_id}"))?;
            play.track_index = index;
            play.position_ms = *position_ms;
            Ok(format!("seek_track {track_id}"))
        }
        ControlAction::SetRepeatMode { repeat_mode } => {
            state.repeat_mode = *repeat_mode;
            Ok(format!("repeat {repeat_mode}"))
        }
        ControlAction::SetShuffleEnabled { shuffle_enabled } => {
            state.shuffle = *shuffle_enabled;
            Ok(format!("shuffle {shuffle_enabled}"))
        }
    }
}

fn insert_track(state: &mut State, track: Track, mode: AddMode) -> String {
    let title = track.title.clone();
    match mode {
        AddMode::PlayNext => {
            let at = (state.play.track_index + 1).min(state.queue.len());
            state.queue.insert(at, track);
            format!("play_next {title} at {at}")
        }
        AddMode::AddToQueue => {
            state.queue.push(track);
            format!("queued {title} at {}", state.queue.len() - 1)
        }
    }
}

fn submit_play_locked(state: &mut State, actor: &str, candidate: PlayState) -> bool {
    if state.play.accept(&candidate) {
        state.push_event(
            ActivityKind::PlayState,
            actor,
            format!("version {}", candidate.version),
        );
        true
    } else {
        false
    }
}

pub struct RunningHost {
    shared: Arc<Mutex<State>>,
    tcp_addr: SocketAddr,
    beacon: BeaconResponder,
    running: Arc<AtomicBool>,
    accept_handle: Option<std::thread::JoinHandle<()>>,
}

impl RunningHost {
    pub fn start(config: HostConfig) -> io::Result<Self> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", config.tcp_port))?;
        listener.set_nonblocking(true)?;
        let tcp_addr = listener.local_addr()?;
        let state = State {
            session_id: config.session_id,
            session_key: config.session_key,
            code: config.code,
            display_name: config.display_name,
            welcome_message: config.welcome_message,
            flags: config.flags,
            authority_id: String::from("host"),
            clients: HashMap::new(),
            order: Vec::new(),
            queue: Vec::new(),
            play: PlayState::default(),
            repeat_mode: 0,
            shuffle: false,
            banned_client_ids: HashSet::new(),
            activity: VecDeque::new(),
            seq: 0,
        };
        let beacon = BeaconResponder::spawn(Beacon::new(
            state.session_id.clone(),
            state.code.0.clone(),
            state.display_name.clone(),
            tcp_addr.port(),
        ))?;
        let shared = Arc::new(Mutex::new(state));
        let running = Arc::new(AtomicBool::new(true));
        let accept_shared = Arc::clone(&shared);
        let accept_running = Arc::clone(&running);
        let accept_handle = std::thread::spawn(move || {
            while accept_running.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let s = Arc::clone(&accept_shared);
                        let r = Arc::clone(&accept_running);
                        std::thread::spawn(move || serve(stream, s, r));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            shared,
            tcp_addr,
            beacon,
            running,
            accept_handle: Some(accept_handle),
        })
    }

    pub fn tcp_addr(&self) -> SocketAddr {
        self.tcp_addr
    }

    pub fn beacon_addr(&self) -> SocketAddr {
        self.beacon.addr()
    }

    pub fn join_info(&self) -> JoinInfo {
        let s = self.shared.lock().expect("state lock poisoned");
        JoinInfo {
            host: String::from("127.0.0.1"),
            port: self.tcp_addr.port(),
            session_id: s.session_id.clone(),
            session_key: s.session_key.clone(),
        }
    }

    pub fn code(&self) -> SessionCode {
        self.shared
            .lock()
            .expect("state lock poisoned")
            .code
            .clone()
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        self.shared.lock().expect("state lock poisoned").snapshot()
    }

    pub fn activity(&self) -> Vec<ActivityEvent> {
        self.shared
            .lock()
            .expect("state lock poisoned")
            .activity
            .iter()
            .cloned()
            .collect()
    }

    pub fn participant(&self, id: &str) -> Option<Participant> {
        self.shared
            .lock()
            .expect("state lock poisoned")
            .snapshot()
            .participants
            .into_iter()
            .find(|p| p.id == id)
    }

    pub fn approve(&self, participant_id: &str, approved: bool) -> bool {
        approve_participant(&self.shared, participant_id, approved)
    }

    pub fn kick(&self, participant_id: &str, reason: Option<&str>) -> bool {
        kick_participant(&self.shared, participant_id, reason)
    }

    pub fn ban(&self, participant_id: &str, reason: Option<&str>) -> bool {
        ban_participant(&self.shared, participant_id, reason)
    }

    pub fn transfer_host(&self, participant_id: &str) -> bool {
        transfer_ownership(&self.shared, participant_id)
    }

    pub fn set_flags(&self, flags: RoomFlags) {
        {
            let mut s = self.shared.lock().expect("state lock poisoned");
            s.flags = flags;
        }
        emit_state(&self.shared);
    }

    pub fn add_track(&self, track: Track, mode: AddMode) -> usize {
        {
            let mut s = self.shared.lock().expect("state lock poisoned");
            let detail = insert_track(&mut s, track, mode);
            s.push_event(ActivityKind::QueueChange, "host", detail);
        }
        emit_state(&self.shared);
        self.snapshot().queue.len()
    }

    pub fn move_queue(&self, from: usize, to: usize) -> bool {
        {
            let mut s = self.shared.lock().expect("state lock poisoned");
            if from >= s.queue.len() || to >= s.queue.len() {
                return false;
            }
            let track = s.queue.remove(from);
            s.queue.insert(to, track);
            s.push_event(
                ActivityKind::QueueChange,
                "host",
                format!("move {from} to {to}"),
            );
        }
        emit_state(&self.shared);
        true
    }

    pub fn remove_at(&self, index: usize) -> Option<Track> {
        let removed = {
            let mut s = self.shared.lock().expect("state lock poisoned");
            if index >= s.queue.len() {
                return None;
            }
            let track = s.queue.remove(index);
            s.push_event(
                ActivityKind::QueueChange,
                "host",
                format!("remove {} at {index}", track.title),
            );
            track
        };
        emit_state(&self.shared);
        Some(removed)
    }

    pub fn submit_play_state(&self, candidate: PlayState) -> bool {
        let accepted = {
            let mut s = self.shared.lock().expect("state lock poisoned");
            submit_play_locked(&mut s, "host", candidate)
        };
        if accepted {
            emit_state(&self.shared);
        }
        accepted
    }

    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.beacon.shutdown();
        let killers: Vec<TcpStream> = match self.shared.lock() {
            Ok(mut s) => {
                let ids: Vec<String> = s.clients.keys().cloned().collect();
                let mut killers = Vec::new();
                for id in ids {
                    if let Some(rec) = s.clients.remove(&id) {
                        killers.push(rec.killer);
                    }
                }
                s.order.clear();
                killers
            }
            Err(_) => Vec::new(),
        };
        for killer in killers {
            let _ = killer.shutdown(Shutdown::Both);
        }
        if let Some(handle) = self.accept_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RunningHost {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

fn is_host_wire(shared: &Arc<Mutex<State>>, participant_id: &str) -> bool {
    shared
        .lock()
        .map(|s| {
            s.clients
                .get(participant_id)
                .is_some_and(|r| !r.pending && r.role == Role::Host)
        })
        .unwrap_or(false)
}

fn close_gracefully(shared: &Arc<Mutex<State>>, participant_id: &str) {
    let sender = match shared.lock() {
        Ok(s) => s.clients.get(participant_id).map(|r| r.sender.clone()),
        Err(_) => return,
    };
    if let Some(sender) = sender {
        push_out(&sender, Out::Close);
    }
}

fn tell(stream: &mut TcpStream, message: &Message) {
    let _ = stream.write_all(frame(message).as_bytes());
    let _ = stream.flush();
}

fn serve(stream: TcpStream, shared: Arc<Mutex<State>>, running: Arc<AtomicBool>) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut reply = match stream.try_clone() {
        Ok(reply) => reply,
        Err(_) => return,
    };
    let pump = match stream.try_clone() {
        Ok(pump) => pump,
        Err(_) => return,
    };
    let killer = match stream.try_clone() {
        Ok(killer) => killer,
        Err(_) => return,
    };
    let (tx, rx) = std::sync::mpsc::sync_channel(MAILBOX);
    spawn_writer(pump, rx);
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let hello = match read_frame(&mut reader, &mut line) {
        Ok(Some(Message::ClientHello {
            protocol_version,
            session_id,
            session_key,
            client_id,
            display_name,
        })) => {
            if protocol_version != PROTOCOL_VERSION {
                tell(
                    &mut reply,
                    &server_error(
                        Some(&session_id),
                        "bad_version",
                        format!("unsupported protocol version {protocol_version}"),
                    ),
                );
                return;
            }
            let valid = match shared.lock() {
                Ok(s) => {
                    session_id == s.session_id
                        && session_key == s.session_key
                        && !s.banned_client_ids.contains(&client_id)
                }
                Err(_) => return,
            };
            if !valid {
                let banned = shared
                    .lock()
                    .map(|s| s.banned_client_ids.contains(&client_id))
                    .unwrap_or(false);
                let (code, message) = if banned {
                    ("banned", String::from("participant is banned"))
                } else {
                    ("bad_session", String::from("invalid session"))
                };
                tell(&mut reply, &server_error(Some(&session_id), code, message));
                return;
            }
            (session_id, client_id, display_name)
        }
        Ok(Some(_)) => {
            tell(
                    &mut reply,
                &server_error(None, "bad_handshake", "handshake required"),
            );
            return;
        }
        Ok(None) => return,
        Err(crate::proto::ProtoError::VersionMismatch(v)) => {
            tell(
                    &mut reply,
                &server_error(
                    None,
                    "bad_version",
                    format!("unsupported protocol version {v}"),
                ),
            );
            return;
        }
        Err(_) => {
            tell(
                    &mut reply,
                &server_error(None, "bad_handshake", "handshake required"),
            );
            return;
        }
    };
    let (session_id, client_id, display_name) = hello;
    let name = display_name.trim();
    let name = if name.is_empty() {
        String::from("Guest")
    } else {
        name.chars().take(64).collect()
    };
    let participant_id = new_id("guest");
    let pending = {
        let mut s = match shared.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let pending = s.flags.require_approval;
        let welcome = Message::ServerWelcome {
            protocol_version: PROTOCOL_VERSION,
            session_id: s.session_id.clone(),
            participant_id: participant_id.clone(),
            role: String::from("guest"),
            pending,
            welcome_message: s.welcome_message.clone(),
            flags: s.flags,
        };
        if !push_frame(&tx, &frame(&welcome)) {
            return;
        }
        s.clients.insert(
            participant_id.clone(),
            ClientRec {
                name: name.clone(),
                client_id: client_id.clone(),
                role: Role::Guest,
                pending,
                sender: tx.clone(),
                killer,
            },
        );
        s.order.push(participant_id.clone());
        s.push_event(
            ActivityKind::Join,
            &participant_id,
            if pending {
                format!("{name} requested to join")
            } else {
                name.clone()
            },
        );
        pending
    };
    let joined = {
        let s = match shared.lock() {
            Ok(s) => s,
            Err(_) => {
                push_out(&tx, Out::Close);
                return;
            }
        };
        Message::ParticipantJoined {
            session_id: s.session_id.clone(),
            participant: Participant {
                id: participant_id.clone(),
                name: name.clone(),
                role: Role::Guest,
                pending,
                connected: true,
            },
        }
    };
    emit(&shared, &joined);
    emit_state(&shared);
    loop {
        match read_frame(&mut reader, &mut line) {
            Ok(Some(message)) => {
                if is_local_timeout_probe(&session_id, &message)
                    && !running.load(Ordering::Relaxed)
                {
                    break;
                }
                if !handle_message(&shared, &tx, &participant_id, &session_id, message) {
                    break;
                }
            }
            Ok(None) => break,
            Err(crate::proto::ProtoError::VersionMismatch(v)) => {
                push_frame(
                    &tx,
                    &frame(&server_error(
                        Some(&session_id),
                        "bad_version",
                        format!("unsupported protocol version {v}"),
                    )),
                );
            }
            Err(e) => {
                push_frame(
                    &tx,
                    &frame(&server_error(
                        Some(&session_id),
                        "bad_frame",
                        e.to_string(),
                    )),
                );
            }
        }
    }
    push_out(&tx, Out::Close);
    remove_client(&shared, &participant_id, "disconnected");
}

fn read_frame(
    reader: &mut BufReader<TcpStream>,
    line: &mut String,
) -> Result<Option<Message>, crate::proto::ProtoError> {
    line.clear();
    match reader.read_line(line) {
        Ok(0) => Ok(None),
        Ok(_) => match decode_line(line) {
            Ok(message) => Ok(Some(message)),
            Err(crate::proto::ProtoError::Empty) => {
                Ok(Some(Message::HeartbeatPing {
                    session_id: String::new(),
                    ping_id: 0,
                }))
            }
            Err(e) => Err(e),
        },
        Err(e)
            if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
        {
            Ok(Some(Message::HeartbeatPing {
                session_id: String::new(),
                ping_id: 0,
            }))
        }
        Err(_) => Ok(None),
    }
}

fn is_local_timeout_probe(session_id: &str, message: &Message) -> bool {
    matches!(message, Message::HeartbeatPing { ping_id: 0, .. }) && session_id.is_empty()
}

fn handle_message(
    shared: &Arc<Mutex<State>>,
    tx: &SyncSender<Out>,
    participant_id: &str,
    session_id: &str,
    message: Message,
) -> bool {
    if let Message::ServerError { code, .. } = &message {
        if code.as_deref() == Some("bad_frame") {
            push_frame(
                tx,
                &frame(&server_error(
                    Some(session_id),
                    "bad_frame",
                    "undecodable frame",
                )),
            );
        }
        return true;
    }
    if is_local_timeout_probe(session_id, &message) {
        return true;
    }
    match message {
        Message::HeartbeatPing { ping_id, .. } => {
            push_frame(
                tx,
                &frame(&Message::HeartbeatPong {
                    session_id: session_id.to_owned(),
                    ping_id,
                }),
            );
            true
        }
        Message::ClientLeave { .. } => false,
        Message::ControlRequest { action, .. } => {
            let outcome = {
                let mut s = match shared.lock() {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                let Some(rec) = s.clients.get(participant_id) else {
                    return false;
                };
                if rec.pending {
                    Err(String::from("join is pending approval"))
                } else if rec.role != Role::Host && !s.flags.guests_can_control {
                    Err(String::from("guests cannot control playback"))
                } else {
                    match apply_control(&mut s, &action) {
                        Ok(detail) => {
                            s.play.version += 1;
                            s.push_event(ActivityKind::PlayState, participant_id, detail);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
            };
            match outcome {
                Ok(()) => emit_state(shared),
                Err(e) => {
                    push_frame(
                        tx,
                        &frame(&server_error(Some(session_id), "control_forbidden", e)),
                    );
                }
            }
            true
        }
        Message::AddTrackRequest { track, mode, .. } => {
            let outcome = {
                let mut s = match shared.lock() {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                let Some(rec) = s.clients.get(participant_id) else {
                    return false;
                };
                if rec.pending {
                    Err(String::from("join is pending approval"))
                } else if rec.role != Role::Host && !s.flags.guests_can_add {
                    Err(String::from("guests cannot add tracks"))
                } else {
                    let detail = insert_track(&mut s, track, mode);
                    s.push_event(ActivityKind::QueueChange, participant_id, detail);
                    Ok(())
                }
            };
            match outcome {
                Ok(()) => emit_state(shared),
                Err(e) => {
                    push_frame(
                        tx,
                        &frame(&server_error(Some(session_id), "add_forbidden", e)),
                    );
                }
            }
            true
        }
        Message::QueueMove { from, to, .. } => {
            let outcome = {
                let mut s = match shared.lock() {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                let Some(rec) = s.clients.get(participant_id) else {
                    return false;
                };
                if rec.pending {
                    Err(String::from("join is pending approval"))
                } else if rec.role != Role::Host && !s.flags.guests_can_add {
                    Err(String::from("guests cannot reorder the queue"))
                } else if from >= s.queue.len() || to >= s.queue.len() {
                    Err(format!("move {from} to {to} out of range"))
                } else {
                    let track = s.queue.remove(from);
                    s.queue.insert(to, track);
                    s.push_event(
                        ActivityKind::QueueChange,
                        participant_id,
                        format!("move {from} to {to}"),
                    );
                    Ok(())
                }
            };
            match outcome {
                Ok(()) => emit_state(shared),
                Err(e) => {
                    push_frame(
                        tx,
                        &frame(&server_error(Some(session_id), "move_forbidden", e)),
                    );
                }
            }
            true
        }
        Message::QueueRemove { index, .. } => {
            let outcome = {
                let mut s = match shared.lock() {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                let Some(rec) = s.clients.get(participant_id) else {
                    return false;
                };
                if rec.pending {
                    Err(String::from("join is pending approval"))
                } else if rec.role != Role::Host && !s.flags.guests_can_add {
                    Err(String::from("guests cannot edit the queue"))
                } else if index >= s.queue.len() {
                    Err(format!("index {index} out of range"))
                } else {
                    let track = s.queue.remove(index);
                    s.push_event(
                        ActivityKind::QueueChange,
                        participant_id,
                        format!("remove {} at {index}", track.title),
                    );
                    Ok(())
                }
            };
            match outcome {
                Ok(()) => emit_state(shared),
                Err(e) => {
                    push_frame(
                        tx,
                        &frame(&server_error(Some(session_id), "remove_forbidden", e)),
                    );
                }
            }
            true
        }
        Message::PlayStateSubmit { play, .. } => {
            let accepted = {
                let mut s = match shared.lock() {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                let Some(rec) = s.clients.get(participant_id) else {
                    return false;
                };
                if rec.pending {
                    false
                } else if rec.role != Role::Host && !s.flags.guests_can_control {
                    push_frame(
                        tx,
                        &frame(&server_error(
                            Some(session_id),
                            "control_forbidden",
                            "guests cannot control playback",
                        )),
                    );
                    return true;
                } else {
                    submit_play_locked(&mut s, participant_id, play)
                }
            };
            if accepted {
                emit_state(shared);
            }
            true
        }
        Message::Kick {
            participant_id: target,
            reason,
            ..
        } => {
            if !is_host_wire(shared, participant_id) {
                push_frame(
                    tx,
                    &frame(&server_error(
                        Some(session_id),
                        "not_host",
                        "only the host moderates",
                    )),
                );
                return true;
            }
            kick_participant(shared, &target, reason.as_deref());
            true
        }
        Message::Ban {
            participant_id: target,
            reason,
            ..
        } => {
            if !is_host_wire(shared, participant_id) {
                push_frame(
                    tx,
                    &frame(&server_error(
                        Some(session_id),
                        "not_host",
                        "only the host moderates",
                    )),
                );
                return true;
            }
            ban_participant(shared, &target, reason.as_deref());
            true
        }
        Message::HostTransfer {
            participant_id: target,
            ..
        } => {
            let authorized = shared
                .lock()
                .map(|s| {
                    s.authority_id == participant_id
                        && s.clients.get(participant_id).is_some_and(|r| !r.pending)
                })
                .unwrap_or(false);
            if !authorized {
                push_frame(
                    tx,
                    &frame(&server_error(
                        Some(session_id),
                        "not_host",
                        "only the host transfers ownership",
                    )),
                );
                return true;
            }
            transfer_ownership(shared, &target);
            true
        }
        _ => {
            push_frame(
                tx,
                &frame(&server_error(
                    Some(session_id),
                    "unexpected",
                    "message not handled here",
                )),
            );
            true
        }
    }
}

fn approve_participant(
    shared: &Arc<Mutex<State>>,
    participant_id: &str,
    approved: bool,
) -> bool {
    let decision = {
        let mut s = match shared.lock() {
            Ok(s) => s,
            Err(_) => return false,
        };
        let rec = match s.clients.get_mut(participant_id) {
            Some(rec) if rec.pending => rec,
            _ => return false,
        };
        let name = rec.name.clone();
        if approved {
            rec.pending = false;
            s.push_event(ActivityKind::Approve, participant_id, name.clone());
            let joined = Message::ParticipantJoined {
                session_id: s.session_id.clone(),
                participant: Participant {
                    id: participant_id.to_owned(),
                    name,
                    role: Role::Guest,
                    pending: false,
                    connected: true,
                },
            };
            let decision = Message::JoinDecision {
                session_id: s.session_id.clone(),
                participant_id: participant_id.to_owned(),
                approved: true,
            };
            (Some(decision), Some(joined))
        } else {
            s.push_event(ActivityKind::Reject, participant_id, name);
            let decision = Message::JoinDecision {
                session_id: s.session_id.clone(),
                participant_id: participant_id.to_owned(),
                approved: false,
            };
            (Some(decision), None)
        }
    };
    let (decision, joined) = decision;
    if let Some(message) = decision {
        send_direct(shared, participant_id, &message);
    }
    if let Some(message) = joined {
        emit(shared, &message);
        emit_state(shared);
    } else {
        close_gracefully(shared, participant_id);
        remove_client(shared, participant_id, "not approved");
    }
    true
}

fn kick_participant(
    shared: &Arc<Mutex<State>>,
    participant_id: &str,
    reason: Option<&str>,
) -> bool {
    remove_with_notice(
        shared,
        participant_id,
        ActivityKind::Kick,
        false,
        reason,
    )
}

fn ban_participant(
    shared: &Arc<Mutex<State>>,
    participant_id: &str,
    reason: Option<&str>,
) -> bool {
    remove_with_notice(shared, participant_id, ActivityKind::Ban, true, reason)
}

fn remove_with_notice(
    shared: &Arc<Mutex<State>>,
    participant_id: &str,
    kind: ActivityKind,
    ban: bool,
    reason: Option<&str>,
) -> bool {
    let existed = {
        let mut s = match shared.lock() {
            Ok(s) => s,
            Err(_) => return false,
        };
        let rec = match s.clients.remove(participant_id) {
            Some(rec) => rec,
            None => return false,
        };
        if ban {
            s.banned_client_ids.insert(rec.client_id.clone());
        }
        s.order.retain(|x| x != participant_id);
        if s.authority_id == participant_id {
            s.authority_id = String::from("host");
        }
        s.push_event(kind, participant_id, rec.name.clone());
        let notice = if ban {
            Message::Ban {
                session_id: s.session_id.clone(),
                participant_id: participant_id.to_owned(),
                reason: reason.map(str::to_owned),
            }
        } else {
            Message::Kick {
                session_id: s.session_id.clone(),
                participant_id: participant_id.to_owned(),
                reason: reason.map(str::to_owned),
            }
        };
        let left = Message::ParticipantLeft {
            session_id: s.session_id.clone(),
            participant_id: participant_id.to_owned(),
            reason: reason.map(str::to_owned),
        };
        (rec.sender, notice, left)
    };
    let (sender, notice, left) = existed;
    push_frame(&sender, &frame(&notice));
    push_out(&sender, Out::Close);
    emit(shared, &left);
    emit_state(shared);
    true
}

fn transfer_ownership(shared: &Arc<Mutex<State>>, participant_id: &str) -> bool {
    let transfer = {
        let mut s = match shared.lock() {
            Ok(s) => s,
            Err(_) => return false,
        };
        for (id, other) in s.clients.iter_mut() {
            other.role = if id == participant_id {
                Role::Host
            } else {
                Role::Guest
            };
        }
        let rec = match s.clients.get_mut(participant_id) {
            Some(rec) if !rec.pending => rec,
            _ => return false,
        };
        let name = rec.name.clone();
        s.authority_id = participant_id.to_owned();
        s.push_event(ActivityKind::Transfer, participant_id, name);
        Message::HostTransferred {
            session_id: s.session_id.clone(),
            participant_id: participant_id.to_owned(),
        }
    };
    emit(shared, &transfer);
    emit_state(shared);
    true
}
