use super::Session;
use fallingsand_protocol::ServerStats;
use fallingsand_server::{Server, ServerConfig, ServerControl};
use std::sync::{Arc, Mutex};

pub(super) struct EmbeddedServer {
    pub(super) control: Arc<ServerControl>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub(super) stats: Arc<Mutex<ServerStats>>,
}

impl Drop for EmbeddedServer {
    fn drop(&mut self) {
        self.control.request_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn launch(world_name: String) -> (Session, EmbeddedServer) {
    let (listener, dialer) = fallingsand_net::memory_listener();
    let control = Arc::new(ServerControl::default());
    let stats = Arc::new(Mutex::new(ServerStats::default()));

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
            if let Err(err) = server.run_blocking(thread_control) {
                bevy::log::error!("embedded server stopped: {err}");
            }
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
