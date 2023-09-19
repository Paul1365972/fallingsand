use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    num::NonZeroUsize,
    sync::mpsc::{channel, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

use itertools::Itertools;
use lru::LruCache;
use rustc_hash::FxHashSet;

struct ChunkManager {
    cache: LruCache<WorldChunkCoords, UnloadedChunk>,
    priority_sender: Sender<WorkTask>,
    background_sender: Sender<WorkTask>,
    handle: JoinHandle<()>,
    closed: bool,
}

enum WorkTask {
    Save(WorldChunkCoords, UnloadedChunk),
    Load(WorldChunkCoords),
    Close,
}

enum WorkResult {
    Saved(WorldChunkCoords, UnloadedChunk),
    Loaded(WorldChunkCoords),
    Pregenerated(WorldChunkCoords),
}

impl ChunkManager {
    fn new() -> Self {
        let (priority_sender, priority_receiver) = channel();
        let (background_sender, background_receiver) = channel();

        let handle = thread::spawn(move || {});
        Self {
            cache: LruCache::new(NonZeroUsize::new(1024).unwrap()),
            priority_sender,
            background_sender,
            handle,
            closed: false,
        }
    }

    fn preload_chunk(&mut self, coords: &WorldChunkCoords) {}

    fn load_chunk(&mut self, coords: &WorldChunkCoords) -> Option<UnloadedChunk> {
        // Spawn off an expensive computation

        let value = self.cache.pop(coords);
        if let Some(chunk) = value {
            return Some(chunk);
        } else {
            None // TODO this is temporary because compile time error bad
        }
    }

    fn save_chunk(&mut self, coords: &WorldChunkCoords, chunk: UnloadedChunk) {}

    fn flush(&mut self) {}
}
