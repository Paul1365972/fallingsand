use super::{ClientGame, Flow, InGame, IoFrame, Phase};
use bevy::log::{error, info, warn};
use fallingsand_net::{Connection, ConnectionStatus};
use fallingsand_protocol::{
    ClientMessage, InputAction, PROTOCOL_VERSION, PlayerId, ServerMessage, Stats, decode_message,
    encode_message,
};

const STALL_SECS: f32 = 2.0;
const RETRY_DELAY: f32 = 2.0;
const RETRY_MAX: f32 = 10.0;
const HANDSHAKE_TIMEOUT_SECS: f32 = 10.0;
#[cfg(not(target_family = "wasm"))]
const DIAL_TIMEOUT_SECS: f32 = 15.0;

fn retry_delay(attempt: u32) -> f32 {
    (RETRY_DELAY * 2f32.powi(attempt.min(8) as i32)).min(RETRY_MAX)
}

pub struct Session {
    conn: Box<dyn Connection>,
    player: Option<PlayerId>,
    pub rx_bytes: u64,
    pub rx_per_sec: u64,
    since_rx: f32,
    handshake_secs: f32,
    window: f32,
    window_bytes: u64,
}

impl Session {
    fn new(conn: Box<dyn Connection>) -> Self {
        let identity = super::identity::load_or_create();
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

    pub fn player(&self) -> Option<PlayerId> {
        self.player
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

#[derive(Default)]
pub struct Supervisor {
    pub target: Option<ConnectTarget>,
    pub attempt: u32,
    pub retry_in: f32,
    pub last_error: Option<String>,
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
        super::identity::arg_value("--world")
    }
    #[cfg(target_family = "wasm")]
    {
        None
    }
}

pub fn default_server() -> String {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    if let Some(server) = super::identity::query_param("server") {
        return server;
    }
    option_env!("FALLINGSAND_SERVER")
        .unwrap_or_default()
        .to_string()
}

pub struct Net {
    pub session: Option<Session>,
    pub supervisor: Supervisor,
    #[cfg(not(target_family = "wasm"))]
    dialing: Option<Dialing>,
    #[cfg(not(target_family = "wasm"))]
    runtime: Option<tokio::runtime::Runtime>,
    #[cfg(not(target_family = "wasm"))]
    embedded: Option<embedded::EmbeddedServer>,
}

impl Net {
    pub fn remote(target: ConnectTarget) -> Self {
        Self {
            session: None,
            supervisor: Supervisor {
                target: Some(target),
                ..Supervisor::default()
            },
            #[cfg(not(target_family = "wasm"))]
            dialing: None,
            #[cfg(not(target_family = "wasm"))]
            runtime: None,
            #[cfg(not(target_family = "wasm"))]
            embedded: None,
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn embedded(world_name: String) -> Self {
        let (session, server) = embedded::launch(world_name);
        Self {
            session: Some(session),
            supervisor: Supervisor::default(),
            dialing: None,
            runtime: None,
            embedded: Some(server),
        }
    }

    pub fn is_embedded(&self) -> bool {
        #[cfg(not(target_family = "wasm"))]
        {
            self.embedded.is_some()
        }
        #[cfg(target_family = "wasm")]
        {
            false
        }
    }

    pub fn embedded_stats(&self) -> Option<Stats> {
        #[cfg(not(target_family = "wasm"))]
        {
            self.embedded
                .as_ref()
                .map(|server| server.stats.lock().unwrap().0)
        }
        #[cfg(target_family = "wasm")]
        {
            None
        }
    }

    pub fn set_embedded_paused(&self, paused: bool) {
        #[cfg(not(target_family = "wasm"))]
        if let Some(server) = &self.embedded {
            server.control.set_paused(paused);
        }
        #[cfg(target_family = "wasm")]
        let _ = paused;
    }

    pub fn request_embedded_save(&self) {
        #[cfg(not(target_family = "wasm"))]
        if let Some(server) = &self.embedded {
            server.control.request_save();
        }
    }
}

pub(super) fn update(game: &mut ClientGame, io: &IoFrame) {
    let Flow::InGame(ingame) = &mut game.flow else {
        return;
    };
    drain(
        ingame,
        io,
        &mut game.changes,
        &mut game.input,
        game.view_prefs.debug_borders,
    );
    supervise(ingame, io.dt, &mut game.changes, &mut game.input);
    sync_debug_stream(ingame, game.view_prefs.debug_borders);
}

fn drain(
    ingame: &mut InGame,
    io: &IoFrame,
    changes: &mut super::Changes,
    input: &mut super::input::InputCore,
    debug_borders: bool,
) {
    let Some(session) = ingame.net.session.as_mut() else {
        return;
    };
    let closed = matches!(session.status(), ConnectionStatus::Closed { .. });
    session.since_rx = if ingame.paused || closed {
        0.0
    } else {
        session.since_rx + io.dt
    };

    while let Some(bytes) = session.conn.poll() {
        session.since_rx = 0.0;
        session.rx_bytes += bytes.len() as u64;
        session.window_bytes += bytes.len() as u64;
        match decode_message::<ServerMessage>(&bytes) {
            Ok(ServerMessage::TickFrame(tick)) => {
                ingame.world.apply(&tick);
                ingame.inventory.apply(&tick, changes);
                ingame
                    .players
                    .apply(&tick, session.player, &mut ingame.you, changes);
                ingame.clock.apply(tick.world_age);
                ingame.debug.track_rects(&tick, debug_borders);
                if ingame.phase == Phase::Connecting && session.player.is_some() {
                    ingame.phase = Phase::Playing;
                }
            }
            Ok(ServerMessage::HelloAck {
                protocol_version,
                player,
                spawn,
            }) => {
                if protocol_version != PROTOCOL_VERSION {
                    error!("server protocol {protocol_version} != {PROTOCOL_VERSION}");
                    session.conn.close("protocol version mismatch");
                } else {
                    session.player = Some(player);
                    info!("joined as {player:?}, spawn {spawn:?}");
                    input.queue(InputAction::SelectSlot(ingame.inventory.selected as u8));
                    input.queue(InputAction::SetBrush(ingame.inventory.brush));
                    ingame.debug.subscribed = false;
                }
            }
            Ok(ServerMessage::Reject { reason }) => {
                error!("server rejected connection: {reason}");
                session.conn.close("rejected");
                ingame.net.supervisor.target = None;
                ingame.net.supervisor.last_error = Some(reason);
            }
            Ok(ServerMessage::PlayerJoined { player, name }) => {
                ingame.players.names.insert(player, name);
                changes.roster = true;
            }
            Ok(ServerMessage::PlayerLeft { player }) => {
                ingame.players.names.remove(&player);
                ingame.players.roster.remove(&player);
                changes.roster = true;
            }
            Ok(ServerMessage::Chat { name, text, .. }) => {
                ingame.chat.push(format!("{name}: {text}"), io.now);
                changes.chat = true;
            }
            Ok(ServerMessage::System { text }) => {
                ingame.chat.push(text, io.now);
                changes.chat = true;
            }
            Err(err) => error!("bad message: {err}"),
        }
    }

    if closed {
        return;
    }

    if session.player.is_none() {
        session.handshake_secs += io.dt;
        if session.handshake_secs > HANDSHAKE_TIMEOUT_SECS {
            warn!("no hello ack after {HANDSHAKE_TIMEOUT_SECS}s");
            session.conn.close("handshake timed out");
        }
    }

    session.window += io.dt;
    if session.window >= 1.0 {
        session.rx_per_sec = (session.window_bytes as f32 / session.window) as u64;
        session.window = 0.0;
        session.window_bytes = 0;
    }
}

fn supervise(
    ingame: &mut InGame,
    dt: f32,
    changes: &mut super::Changes,
    input: &mut super::input::InputCore,
) {
    let status = ingame.net.session.as_ref().map(Session::status);
    match status {
        Some(ConnectionStatus::Closed { reason }) => {
            warn!("connection closed: {reason}");
            ingame.net.session = None;
            ingame.on_session_lost(changes, input);
            let supervisor = &mut ingame.net.supervisor;
            supervisor.retry_in = retry_delay(supervisor.attempt);
            if supervisor.target.is_some() || supervisor.last_error.is_none() {
                supervisor.last_error = Some(reason);
            }
        }
        Some(ConnectionStatus::Connected) => {
            if ingame
                .net
                .session
                .as_ref()
                .is_some_and(|session| session.player.is_some())
            {
                let supervisor = &mut ingame.net.supervisor;
                if supervisor.attempt != 0 || supervisor.last_error.is_some() {
                    supervisor.attempt = 0;
                    supervisor.last_error = None;
                }
            }
        }
        None => {
            if poll_dial(&mut ingame.net, dt) {
                return;
            }
            let Some(target) = ingame.net.supervisor.target.clone() else {
                return;
            };
            let supervisor = &mut ingame.net.supervisor;
            supervisor.retry_in -= dt;
            if supervisor.retry_in > 0.0 {
                return;
            }
            supervisor.attempt += 1;
            supervisor.retry_in = retry_delay(supervisor.attempt);
            let attempt = supervisor.attempt;
            info!("connecting to {} (attempt {attempt})", target.url);
            start_dial(&mut ingame.net, target);
        }
    }
}

fn sync_debug_stream(ingame: &mut InGame, debug_borders: bool) {
    let Some(session) = ingame.net.session.as_mut() else {
        ingame.debug.subscribed = false;
        return;
    };
    if session.player.is_some() && ingame.debug.subscribed != debug_borders {
        session.send(&ClientMessage::SetDebug {
            enabled: debug_borders,
        });
        ingame.debug.subscribed = debug_borders;
    }
}

#[cfg(not(target_family = "wasm"))]
struct Dialing {
    receiver: std::sync::Mutex<std::sync::mpsc::Receiver<Result<Box<dyn Connection>, String>>>,
    elapsed: f32,
}

#[cfg(not(target_family = "wasm"))]
fn poll_dial(net: &mut Net, dt: f32) -> bool {
    let Some(dialing) = net.dialing.as_mut() else {
        return false;
    };
    dialing.elapsed += dt;
    let timed_out = dialing.elapsed > DIAL_TIMEOUT_SECS;
    let result = dialing.receiver.lock().unwrap().try_recv();
    match result {
        Ok(Ok(conn)) => {
            net.dialing = None;
            net.session = Some(Session::new(conn));
        }
        Ok(Err(err)) => {
            net.dialing = None;
            error!("failed to connect: {err}");
            net.supervisor.last_error = Some(err);
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            if timed_out {
                net.dialing = None;
                error!("connect attempt timed out after {DIAL_TIMEOUT_SECS}s");
                net.supervisor.last_error = Some("connect timed out".into());
            }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            net.dialing = None;
            net.supervisor.last_error = Some("connect worker died".into());
        }
    }
    net.dialing.is_some()
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn poll_dial(_net: &mut Net, _dt: f32) -> bool {
    false
}

#[cfg(not(target_family = "wasm"))]
fn start_dial(net: &mut Net, target: ConnectTarget) {
    if net.runtime.is_none() {
        match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => net.runtime = Some(runtime),
            Err(err) => {
                net.supervisor.last_error = Some(err.to_string());
                return;
            }
        }
    }
    let handle = net.runtime.as_ref().unwrap().handle().clone();
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
    net.dialing = Some(Dialing {
        receiver: std::sync::Mutex::new(receiver),
        elapsed: 0.0,
    });
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn start_dial(net: &mut Net, target: ConnectTarget) {
    let conn = Box::new(fallingsand_net::wt_wasm::connect(
        &target.url,
        target.cert_hash.clone(),
    ));
    net.session = Some(Session::new(conn));
}

#[cfg(not(target_family = "wasm"))]
mod embedded {
    use super::Session;
    use fallingsand_server::{Server, ServerConfig, ServerControl, TickStats};
    use std::sync::{Arc, Mutex};

    pub struct EmbeddedServer {
        pub control: Arc<ServerControl>,
        thread: Option<std::thread::JoinHandle<()>>,
        pub stats: Arc<Mutex<TickStats>>,
    }

    impl Drop for EmbeddedServer {
        fn drop(&mut self) {
            self.control.request_stop();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    pub fn launch(world_name: String) -> (Session, EmbeddedServer) {
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

        let session = Session::new(dialer.connect().expect("connect to embedded server"));
        let server = EmbeddedServer {
            control,
            thread: Some(thread),
            stats,
        };
        (session, server)
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
}
