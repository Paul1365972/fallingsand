use crate::ClientRegistry;
use bevy::prelude::*;
use fallingsand_core::CellPos;
use fallingsand_net::{Connection, ConnectionStatus};
use fallingsand_protocol::{
    ClientMessage, PROTOCOL_VERSION, PlayerId, ServerMessage, decode_message, encode_message,
};

pub struct NetPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetSet;

#[derive(Resource)]
pub struct Conn(pub Box<dyn Connection>);

#[derive(Resource, Default)]
pub struct LocalPlayer {
    pub id: Option<PlayerId>,
    pub spawn: Option<CellPos>,
}

#[derive(Resource, Default)]
pub struct NetStats {
    pub rx_bytes: u64,
    pub rx_per_sec: u64,
    window: f32,
    window_bytes: u64,
}

#[derive(Resource, Default, Clone, Copy)]
pub struct EmbeddedServerStats {
    pub tick: u64,
    pub sim_micros: u64,
    pub awake_chunks: usize,
    pub loaded_chunks: usize,
    pub players: usize,
    pub replicated_bytes: u64,
    pub pixel_bodies: usize,
}

#[derive(Message)]
pub struct ServerMsg(pub ServerMessage);

pub struct ConnectTarget {
    pub url: String,
    pub cert_hash: Option<Vec<u8>>,
}

pub fn parse_cert_hash(hex: &str) -> Option<Vec<u8>> {
    if hex.len() != 64 || !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

#[derive(Resource, Default)]
pub struct PendingConnect(pub Option<ConnectTarget>);

pub fn cli_world_name() -> Option<String> {
    #[cfg(not(target_family = "wasm"))]
    {
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--world" {
                return args.next();
            }
        }
        None
    }
    #[cfg(target_family = "wasm")]
    {
        None
    }
}

pub fn cli_connect_target() -> Option<ConnectTarget> {
    #[cfg(not(target_family = "wasm"))]
    {
        let mut args = std::env::args().skip(1);
        let mut target: Option<ConnectTarget> = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--connect" => {
                    if let Some(url) = args.next() {
                        target = Some(ConnectTarget {
                            url,
                            cert_hash: None,
                        });
                    }
                }
                "--cert-hash" => {
                    if let (Some(target), Some(hex)) = (target.as_mut(), args.next()) {
                        target.cert_hash = parse_cert_hash(&hex);
                        if target.cert_hash.is_none() {
                            error!("invalid --cert-hash, expected 64 hex chars");
                        }
                    }
                }
                _ => {}
            }
        }
        target
    }
    #[cfg(target_family = "wasm")]
    {
        wasm_remote::query_connect_target()
    }
}

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalPlayer>()
            .init_resource::<NetStats>()
            .init_resource::<EmbeddedServerStats>()
            .init_resource::<PendingConnect>()
            .add_message::<ServerMsg>()
            .add_systems(OnEnter(crate::AppState::InGame), connect)
            .add_systems(OnExit(crate::AppState::InGame), disconnect)
            .add_systems(
                PreUpdate,
                drain_connection
                    .in_set(NetSet)
                    .run_if(resource_exists::<Conn>),
            );

        #[cfg(not(target_family = "wasm"))]
        app.add_systems(
            PreUpdate,
            embedded::mirror_stats
                .before(NetSet)
                .run_if(resource_exists::<embedded::EmbeddedServer>),
        );
    }
}

fn connect(world: &mut World) {
    if world.contains_resource::<Conn>() {
        return;
    }
    let target = world.resource_mut::<PendingConnect>().0.take();
    #[cfg(not(target_family = "wasm"))]
    match target {
        Some(target) => remote::setup(world, target),
        None => embedded::setup(world),
    }
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    if let Some(target) = target {
        wasm_remote::setup(world, target);
    } else {
        warn!("no ?server=https://host:port query parameter; not connecting");
    }
}

fn disconnect(world: &mut World) {
    if let Some(mut conn) = world.remove_resource::<Conn>() {
        conn.0.send(encode_message(&ClientMessage::Goodbye));
    }
    #[cfg(not(target_family = "wasm"))]
    {
        world.remove_resource::<embedded::EmbeddedServer>();
        world.remove_resource::<remote::RemoteRuntime>();
    }
    *world.resource_mut::<LocalPlayer>() = LocalPlayer::default();
    *world.resource_mut::<NetStats>() = NetStats::default();
    *world.resource_mut::<EmbeddedServerStats>() = EmbeddedServerStats::default();
}

fn drain_connection(
    mut conn: ResMut<Conn>,
    mut local: ResMut<LocalPlayer>,
    mut stats: ResMut<NetStats>,
    mut messages: MessageWriter<ServerMsg>,
    registry: Res<ClientRegistry>,
    time: Res<Time>,
) {
    if let ConnectionStatus::Closed { reason } = conn.0.status() {
        warn!("connection closed: {reason}");
        return;
    }

    while let Some(bytes) = conn.0.poll() {
        stats.rx_bytes += bytes.len() as u64;
        stats.window_bytes += bytes.len() as u64;
        match decode_message::<ServerMessage>(&bytes) {
            Ok(message) => {
                if let ServerMessage::HelloAck {
                    protocol_version,
                    registry_hash,
                    player,
                    spawn,
                    ..
                } = &message
                {
                    if *protocol_version != PROTOCOL_VERSION {
                        error!("server protocol {protocol_version} != {PROTOCOL_VERSION}");
                    }
                    if *registry_hash != registry.0.hash() {
                        error!("material registry hash mismatch with server");
                    }
                    local.id = Some(*player);
                    local.spawn = Some(*spawn);
                    info!("joined as {player:?}, spawn {spawn:?}");
                }
                messages.write(ServerMsg(message));
            }
            Err(err) => error!("bad message: {err}"),
        }
    }

    stats.window += time.delta_secs();
    if stats.window >= 1.0 {
        stats.rx_per_sec = (stats.window_bytes as f32 / stats.window) as u64;
        stats.window = 0.0;
        stats.window_bytes = 0;
    }
}

#[cfg(not(target_family = "wasm"))]
mod remote {
    use super::*;

    pub fn setup(world: &mut World, target: ConnectTarget) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let handle = runtime.handle().clone();
        match fallingsand_net::wt_native::connect(handle, &target.url, target.cert_hash) {
            Ok(mut conn) => {
                info!("connected to {}", target.url);
                conn.send(encode_message(&ClientMessage::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    name: player_name(),
                }));
                world.insert_resource(Conn(Box::new(conn)));
                world.insert_resource(RemoteRuntime(runtime));
            }
            Err(err) => {
                error!("failed to connect to {}: {err}", target.url);
            }
        }
    }

    #[derive(Resource)]
    pub(super) struct RemoteRuntime(#[allow(dead_code)] tokio::runtime::Runtime);
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
mod wasm_remote {
    use super::*;

    pub fn query_connect_target() -> Option<ConnectTarget> {
        let location = web_sys::window().map(|w| w.location());
        let query = location.and_then(|l| l.search().ok()).unwrap_or_default();
        let mut url = None;
        let mut cert_hash = None;
        for pair in query.trim_start_matches('?').split('&') {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some("server"), Some(value)) => {
                    url = js_sys::decode_uri_component(value)
                        .ok()
                        .map(|v| String::from(v));
                }
                (Some("cert"), Some(value)) => cert_hash = parse_cert_hash(value),
                _ => {}
            }
        }
        url.map(|url| ConnectTarget { url, cert_hash })
    }

    pub fn setup(world: &mut World, target: ConnectTarget) {
        let mut conn = fallingsand_net::wt_wasm::connect(&target.url, target.cert_hash);
        info!("connecting to {}", target.url);
        conn.send(encode_message(&ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            name: player_name(),
        }));
        world.insert_resource(Conn(Box::new(conn)));
    }
}

#[cfg(not(target_family = "wasm"))]
fn player_name() -> String {
    std::env::var("FS_PLAYER").unwrap_or_else(|_| format!("player{}", std::process::id() % 1000))
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn player_name() -> String {
    format!("web{}", (js_sys::Math::random() * 1000.0) as u32)
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

    pub fn setup(world: &mut World) {
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
                        biomes_source: crate::BIOMES_RON.to_string(),
                    },
                })
                .expect("embedded server init");
                server.run_blocking(thread_control);
            })
            .expect("spawn embedded server thread");

        let mut conn = dialer.connect().expect("connect to embedded server");
        conn.send(encode_message(&ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            name: "local".into(),
        }));

        world.insert_resource(Conn(Box::new(conn)));
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

    pub fn mirror_stats(server: Res<EmbeddedServer>, mut mirror: ResMut<EmbeddedServerStats>) {
        let stats = *server.stats.lock().unwrap();
        *mirror = EmbeddedServerStats {
            tick: stats.tick,
            sim_micros: stats.sim_micros,
            awake_chunks: stats.awake_chunks,
            loaded_chunks: stats.loaded_chunks,
            players: stats.players,
            replicated_bytes: stats.replicated_bytes,
            pixel_bodies: stats.pixel_bodies,
        };
    }
}
