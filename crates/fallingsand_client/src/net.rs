#[cfg(not(target_family = "wasm"))]
use crate::ClientRegistry;
use bevy::prelude::*;
use fallingsand_net::{Connection, ConnectionStatus};
use fallingsand_protocol::{
    ClientMessage, PROTOCOL_VERSION, PlayerId, ServerMessage, Stats, TickFrame, decode_message,
    encode_message,
};

pub struct NetPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetSet;

const STALL_SECS: f32 = 2.0;
const RETRY_DELAY: f32 = 2.0;
const RETRY_MAX: f32 = 10.0;
const HANDSHAKE_TIMEOUT_SECS: f32 = 10.0;
#[cfg(not(target_family = "wasm"))]
const DIAL_TIMEOUT_SECS: f32 = 15.0;

fn retry_delay(attempt: u32) -> f32 {
    (RETRY_DELAY * 2f32.powi(attempt.min(8) as i32)).min(RETRY_MAX)
}

#[derive(Resource)]
pub struct Session {
    conn: Box<dyn Connection>,
    pub player: Option<PlayerId>,
    pub rx_bytes: u64,
    pub rx_per_sec: u64,
    since_rx: f32,
    handshake_secs: f32,
    window: f32,
    window_bytes: u64,
}

impl Session {
    fn new(conn: Box<dyn Connection>, identity: crate::identity::Identity) -> Self {
        let mut session = Self {
            conn,
            player: None,
            rx_bytes: 0,
            rx_per_sec: 0,
            since_rx: 0.0,
            handshake_secs: 0.0,
            window: 0.0,
            window_bytes: 0,
        };
        session.send(&ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            uuid: identity.uuid,
            name: identity.name,
        });
        session
    }

    pub fn send(&mut self, message: &ClientMessage) {
        self.conn.send(encode_message(message));
    }

    fn status(&self) -> ConnectionStatus {
        self.conn.status()
    }
}

#[derive(Clone)]
pub struct ConnectTarget {
    pub url: String,
    pub cert_hash: Option<Vec<u8>>,
}

#[derive(Resource, Default)]
pub struct Supervisor {
    pub target: Option<ConnectTarget>,
    pub attempt: u32,
    pub retry_in: f32,
    pub last_error: Option<String>,
}

impl Supervisor {
    pub fn new(target: Option<ConnectTarget>) -> Self {
        Self {
            target,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConnPhase {
    Connecting,
    Reconnecting { attempt: u32 },
    Online,
    Stalled { seconds: f32 },
    Lost { reason: String },
}

impl Supervisor {
    pub fn phase(&self, session: Option<&Session>, paused: bool) -> ConnPhase {
        match session {
            Some(session) => {
                if session.player.is_none() {
                    if self.last_error.is_none() && self.attempt <= 1 {
                        ConnPhase::Connecting
                    } else {
                        ConnPhase::Reconnecting {
                            attempt: self.attempt.max(1),
                        }
                    }
                } else if !paused && session.since_rx >= STALL_SECS {
                    ConnPhase::Stalled {
                        seconds: session.since_rx,
                    }
                } else {
                    ConnPhase::Online
                }
            }
            None if self.target.is_some() => {
                if self.last_error.is_none() && self.attempt <= 1 {
                    ConnPhase::Connecting
                } else {
                    ConnPhase::Reconnecting {
                        attempt: self.attempt.max(1),
                    }
                }
            }
            None => ConnPhase::Lost {
                reason: self
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "not connected".into()),
            },
        }
    }
}

#[derive(Message)]
pub struct ServerMsg(pub ServerMessage);

#[derive(Message)]
pub struct TickMessage(pub TickFrame);

#[derive(Message)]
pub struct SessionEnded;

#[derive(Resource, Default, Clone, Copy, PartialEq)]
pub struct ServerStats(pub Stats);

impl std::ops::Deref for ServerStats {
    type Target = Stats;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn parse_cert_hash(hex: &str) -> Option<Vec<u8>> {
    if hex.len() != 64 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

pub fn cli_world_name() -> Option<String> {
    #[cfg(not(target_family = "wasm"))]
    {
        crate::identity::arg_value("--world")
    }
    #[cfg(target_family = "wasm")]
    {
        None
    }
}

pub fn cli_connect_target() -> Option<ConnectTarget> {
    #[cfg(not(target_family = "wasm"))]
    {
        let url = crate::identity::arg_value("--connect")?;
        let cert_hash = crate::identity::arg_value("--cert-hash").and_then(|hex| {
            let hash = parse_cert_hash(&hex);
            if hash.is_none() {
                error!("invalid --cert-hash, expected 64 hex chars");
            }
            hash
        });
        Some(ConnectTarget { url, cert_hash })
    }
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        let url = crate::identity::query_param("server")?;
        let cert_hash = crate::identity::query_param("cert").and_then(|hex| parse_cert_hash(&hex));
        Some(ConnectTarget { url, cert_hash })
    }
}

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Supervisor>()
            .init_resource::<ServerStats>()
            .add_message::<ServerMsg>()
            .add_message::<TickMessage>()
            .add_message::<SessionEnded>()
            .add_systems(OnEnter(crate::AppState::InGame), open)
            .add_systems(OnExit(crate::AppState::InGame), close)
            .add_systems(
                PreUpdate,
                drain.in_set(NetSet).run_if(resource_exists::<Session>),
            )
            .add_systems(
                PreUpdate,
                supervise
                    .after(NetSet)
                    .run_if(in_state(crate::AppState::InGame)),
            )
            .add_systems(
                PreUpdate,
                enter_playing
                    .after(NetSet)
                    .run_if(in_state(crate::GameState::Connecting)),
            );

        #[cfg(not(target_family = "wasm"))]
        app.add_systems(PreUpdate, embedded::mirror_stats.before(NetSet));
    }
}

fn open(world: &mut World) {
    let target = world.resource::<Supervisor>().target.clone();
    *world.resource_mut::<Supervisor>() = Supervisor::new(target);
    if world.resource::<Supervisor>().target.is_none() {
        #[cfg(not(target_family = "wasm"))]
        embedded::launch(world);
        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
        warn!("no ?server=host query parameter; not connecting");
    }
}

fn close(world: &mut World) {
    if let Some(mut session) = world.remove_resource::<Session>() {
        session.send(&ClientMessage::Goodbye);
    }
    *world.resource_mut::<Supervisor>() = Supervisor::default();
    #[cfg(not(target_family = "wasm"))]
    world.remove_resource::<embedded::EmbeddedServer>();
    #[cfg(not(target_family = "wasm"))]
    world.remove_resource::<Dialing>();
}

#[allow(clippy::too_many_arguments)]
fn drain(
    mut session: ResMut<Session>,
    mut supervisor: ResMut<Supervisor>,
    mut messages: MessageWriter<ServerMsg>,
    mut frames: MessageWriter<TickMessage>,
    pause: Option<Res<State<crate::PauseState>>>,
    time: Res<Time>,
) {
    let closed = matches!(session.status(), ConnectionStatus::Closed { .. });
    let paused = pause.is_some_and(|state| *state.get() == crate::PauseState::Paused);
    session.since_rx = if paused || closed {
        0.0
    } else {
        session.since_rx + time.delta_secs()
    };

    while let Some(bytes) = session.conn.poll() {
        session.since_rx = 0.0;
        session.rx_bytes += bytes.len() as u64;
        session.window_bytes += bytes.len() as u64;
        match decode_message::<ServerMessage>(&bytes) {
            Ok(ServerMessage::TickFrame(tick)) => {
                frames.write(TickMessage(tick));
            }
            Ok(message) => {
                if let ServerMessage::HelloAck {
                    protocol_version,
                    player,
                    spawn,
                } = &message
                {
                    if *protocol_version != PROTOCOL_VERSION {
                        error!("server protocol {protocol_version} != {PROTOCOL_VERSION}");
                        session.conn.close("protocol version mismatch");
                    } else {
                        session.player = Some(*player);
                        info!("joined as {player:?}, spawn {spawn:?}");
                    }
                }
                if let ServerMessage::Reject { reason } = &message {
                    error!("server rejected connection: {reason}");
                    supervisor.target = None;
                    supervisor.last_error = Some(reason.clone());
                }
                messages.write(ServerMsg(message));
            }
            Err(err) => error!("bad message: {err}"),
        }
    }

    if closed {
        return;
    }

    if session.player.is_none() {
        session.handshake_secs += time.delta_secs();
        if session.handshake_secs > HANDSHAKE_TIMEOUT_SECS {
            warn!("no hello ack after {HANDSHAKE_TIMEOUT_SECS}s");
            session.conn.close("handshake timed out");
        }
    }

    session.window += time.delta_secs();
    if session.window >= 1.0 {
        session.rx_per_sec = (session.window_bytes as f32 / session.window) as u64;
        session.window = 0.0;
        session.window_bytes = 0;
    }
}

fn enter_playing(
    session: Option<Res<Session>>,
    mut frames: MessageReader<TickMessage>,
    mut next: ResMut<NextState<crate::GameState>>,
) {
    let joined = session.is_some_and(|session| session.player.is_some());
    let got_frame = frames.read().next().is_some();
    if joined && got_frame {
        next.set(crate::GameState::Playing);
    }
}

fn supervise(world: &mut World) {
    let status = world.get_resource::<Session>().map(Session::status);
    match status {
        Some(ConnectionStatus::Closed { reason }) => {
            warn!("connection closed: {reason}");
            world.remove_resource::<Session>();
            world.write_message(SessionEnded);
            let mut supervisor = world.resource_mut::<Supervisor>();
            supervisor.retry_in = retry_delay(supervisor.attempt);
            if supervisor.target.is_some() || supervisor.last_error.is_none() {
                supervisor.last_error = Some(reason);
            }
        }
        Some(ConnectionStatus::Connected) => {
            if world.resource::<Session>().player.is_some() {
                let mut supervisor = world.resource_mut::<Supervisor>();
                if supervisor.attempt != 0 || supervisor.last_error.is_some() {
                    supervisor.attempt = 0;
                    supervisor.last_error = None;
                }
            }
        }
        None => {
            if poll_dial(world) {
                return;
            }
            let Some(target) = world.resource::<Supervisor>().target.clone() else {
                return;
            };
            let delta = world.resource::<Time>().delta_secs();
            let mut supervisor = world.resource_mut::<Supervisor>();
            supervisor.retry_in -= delta;
            if supervisor.retry_in > 0.0 {
                return;
            }
            supervisor.attempt += 1;
            supervisor.retry_in = retry_delay(supervisor.attempt);
            let attempt = supervisor.attempt;
            info!("connecting to {} (attempt {attempt})", target.url);
            start_dial(world, target);
        }
    }
}

#[cfg(not(target_family = "wasm"))]
#[derive(Resource)]
struct NetRuntime(tokio::runtime::Runtime);

#[cfg(not(target_family = "wasm"))]
#[derive(Resource)]
struct Dialing {
    receiver: std::sync::Mutex<std::sync::mpsc::Receiver<Result<Box<dyn Connection>, String>>>,
    elapsed: f32,
}

#[cfg(not(target_family = "wasm"))]
fn poll_dial(world: &mut World) -> bool {
    let delta = world.resource::<Time>().delta_secs();
    let Some(mut dialing) = world.get_resource_mut::<Dialing>() else {
        return false;
    };
    dialing.elapsed += delta;
    let timed_out = dialing.elapsed > DIAL_TIMEOUT_SECS;
    let result = dialing.receiver.lock().unwrap().try_recv();
    match result {
        Ok(Ok(conn)) => {
            world.remove_resource::<Dialing>();
            world.insert_resource(Session::new(conn, crate::identity::load_or_create()));
        }
        Ok(Err(err)) => {
            world.remove_resource::<Dialing>();
            error!("failed to connect: {err}");
            world.resource_mut::<Supervisor>().last_error = Some(err);
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            if timed_out {
                world.remove_resource::<Dialing>();
                error!("connect attempt timed out after {DIAL_TIMEOUT_SECS}s");
                world.resource_mut::<Supervisor>().last_error = Some("connect timed out".into());
            }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            world.remove_resource::<Dialing>();
            world.resource_mut::<Supervisor>().last_error = Some("connect worker died".into());
        }
    }
    world.contains_resource::<Dialing>()
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn poll_dial(_world: &mut World) -> bool {
    false
}

#[cfg(not(target_family = "wasm"))]
fn start_dial(world: &mut World, target: ConnectTarget) {
    if !world.contains_resource::<NetRuntime>() {
        match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => world.insert_resource(NetRuntime(runtime)),
            Err(err) => {
                world.resource_mut::<Supervisor>().last_error = Some(err.to_string());
                return;
            }
        }
    }
    let handle = world.resource::<NetRuntime>().0.handle().clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("wt-dial".into())
        .spawn(move || {
            let result = fallingsand_net::wt_native::connect(handle, &target.url, target.cert_hash)
                .map(|conn| Box::new(conn) as Box<dyn Connection>)
                .map_err(|err| err.to_string());
            let _ = sender.send(result);
        })
        .expect("spawn dial thread");
    world.insert_resource(Dialing {
        receiver: std::sync::Mutex::new(receiver),
        elapsed: 0.0,
    });
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn start_dial(world: &mut World, target: ConnectTarget) {
    let conn = Box::new(fallingsand_net::wt_wasm::connect(
        &target.url,
        target.cert_hash.clone(),
    ));
    world.insert_resource(Session::new(conn, crate::identity::load_or_create()));
}

#[cfg(not(target_family = "wasm"))]
pub mod embedded {
    use super::*;
    use fallingsand_server::{Server, ServerConfig, ServerControl, TickStats};
    use std::sync::{Arc, Mutex};

    #[derive(Resource)]
    pub struct EmbeddedServer {
        pub control: Arc<ServerControl>,
        thread: Option<std::thread::JoinHandle<()>>,
        stats: Arc<Mutex<TickStats>>,
    }

    impl Drop for EmbeddedServer {
        fn drop(&mut self) {
            self.control.request_stop();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    pub fn launch(world: &mut World) {
        let registry = world.resource::<ClientRegistry>().0.clone();
        let world_name = world.resource::<crate::menu::SelectedWorld>().0.clone();
        let (listener, dialer) = fallingsand_net::memory_listener();
        let control = Arc::new(ServerControl::default());
        let stats = Arc::new(Mutex::new(TickStats::default()));

        let thread_control = control.clone();
        let thread_stats = stats.clone();
        let thread = std::thread::Builder::new()
            .name("embedded-server".into())
            .spawn(move || {
                let save_path = std::path::Path::new("saves")
                    .join(&world_name)
                    .join("world.redb");
                let mut server = Server::new(ServerConfig {
                    registry,
                    listener: Box::new(listener),
                    stats_sink: Some(thread_stats),
                    world: fallingsand_server::WorldConfig {
                        seed: derive_seed(&world_name),
                        name: world_name,
                        save_path: Some(save_path),
                    },
                })
                .expect("embedded server init");
                server.run_blocking(thread_control);
            })
            .expect("spawn embedded server thread");

        let conn = dialer.connect().expect("connect to embedded server");
        world.insert_resource(Session::new(
            Box::new(conn),
            crate::identity::load_or_create(),
        ));
        world.insert_resource(EmbeddedServer {
            control,
            thread: Some(thread),
            stats,
        });
    }

    fn derive_seed(name: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut hasher);
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
            .hash(&mut hasher);
        hasher.finish()
    }

    pub fn mirror_stats(server: Option<Res<EmbeddedServer>>, mut mirror: ResMut<ServerStats>) {
        let next = server
            .map(|server| ServerStats(server.stats.lock().unwrap().0))
            .unwrap_or_default();
        mirror.set_if_neq(next);
    }
}
