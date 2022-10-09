use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    time::Duration,
};

use itertools::Itertools;
use rustc_hash::FxHashSet;

use crate::{cell::cell::TileTransitionFn, coords::WorldChunkCoords, world::{World, Entity}};

enum OrchestratorMode {
    SINGLEPLAYER,
    MULTIPLAYER(TcpStream),
    HOST(TcpListener, Vec<Connection>),
}

impl OrchestratorMode {
    fn new_host(addr: &SocketAddr) -> OrchestratorMode {
        let listener = TcpListener::bind(addr).unwrap();
        listener.set_nonblocking(true).unwrap();
        OrchestratorMode::HOST(listener, Vec::new())
    }

    fn new_multiplayer(addr: &SocketAddr) -> OrchestratorMode {
        let stream = TcpStream::connect_timeout(addr, Duration::new(10, 0)).unwrap();
        stream.set_nodelay(true).unwrap();
        stream.set_nonblocking(true).unwrap();
        OrchestratorMode::MULTIPLAYER(stream)
    }
}

pub struct Connection {
    stream: TcpStream,
}

pub struct Orchestrator<T, E> {
    mode: OrchestratorMode,
    world: World<T, E>,
}

impl<T: Send, E: Entity> Orchestrator<T, E> {
    fn accept_connections(&mut self) {
        match self.mode {
            OrchestratorMode::HOST(listener, connections) => {
                if let Ok((stream, addr)) = listener.accept() {
                    connections.push(Connection { stream: stream });
                }
            }
            _ => panic!("Called accept connections on Client"),
        }
    }

    fn handle_connctions(&mut self) {
        match self.mode {
            OrchestratorMode::SINGLEPLAYER => todo!(),
            OrchestratorMode::MULTIPLAYER(_) => todo!(),
            OrchestratorMode::HOST(_, _) => todo!(),
        }
    }

    pub fn step<F>(&mut self, tile_transition_fn: F)
    where
        F: TileTransitionFn<T>,
    {
        self.accept_connections();
        self.handle_connctions();
        self.world.step(tile_transition_fn);
    }
}

pub struct ViewReceiver {
    id: u32, // ???
    coords: WorldChunkCoords,
    loaded_chunks: FxHashSet<WorldChunkCoords>,
}

impl ViewReceiver {
    fn update_view<T: Send, E: Entity>(
        &mut self,
        world: &World<T, E>,
        // updated_chunks: Option<FxHashSet<WorldChunkCoords>>,
    ) {
        let region = world
            .get_region(&self.coords)
            .expect("ViewReceiver in chunk with no assigned region");
        // let updated_chunks = updated_chunks.unwrap_or_else(|| {
        // let mut set = receiver.loaded_chunks.clone();
        // set.extend(region.chunks_iter().map(|(&k, v)| k));
        // set
        // });

        let to_load = region
            .chunks_iter()
            .filter(|(x, _)| !self.loaded_chunks.contains(x))
            .collect_vec();
        let to_unload = self
            .loaded_chunks
            .iter()
            .filter(|x| !region.contains_chunk(x))
            .collect_vec();
        self.loaded_chunks.clear();
        self.loaded_chunks
            .extend(region.chunks_iter().map(|(x, _)| x));
    }
}
