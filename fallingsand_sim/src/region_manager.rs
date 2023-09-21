use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    num::NonZeroUsize,
    sync::{mpsc::{channel, Sender}, Mutex, Arc},
    thread::{self, JoinHandle},
    time::Duration, collections::BinaryHeap,
};

use hashlink::LinkedHashMap;
use itertools::Itertools;
use rustc_hash::FxHashSet;

use crate::{chunk::UnloadedRegion, util::coords::WorldRegionCoords};

struct ChunkManager {
    cache: LinkedHashMap<WorldRegionCoords, UnloadedRegion>,
    handle: JoinHandle<()>,
    closed: bool,
    work: Arc<Mutex<WorkQueue>>,
}

struct WorkQueue {
    load: Vec<WorldRegionCoords>,
    preload: Vec<WorldRegionCoords>,
}

// enum WorkTask {
//     Save(WorldChunkCoords, UnloadedChunk),
//     Load(WorldChunkCoords),
//     Close,
// }
// 
// enum WorkResult {
//     Saved(WorldChunkCoords, UnloadedChunk),
//     Loaded(WorldChunkCoords),
//     Pregenerated(WorldChunkCoords),
// }

impl ChunkManager {
    // fn new() -> Self {
    //     let handle = thread::spawn(move || {});
    //     Self {
    //         cache: LruCache::new(NonZeroUsize::new(1024).unwrap()),
    //         priority_sender,
    //         background_sender,
    //         handle,
    //         closed: false,
    //     }
    // }

    fn preload_chunk(&mut self, coords: &WorldRegionCoords) {}

    fn load_chunk(&mut self, coords: &WorldRegionCoords) -> Option<UnloadedRegion> {
        // Spawn off an expensive computation

        let value = self.cache.remove(coords);
        if let Some(chunk) = value {
            return Some(chunk);
        } else {
            None // TODO this is temporary because compile time error bad
        }
    }

    fn save_chunk(&mut self, coords: &WorldRegionCoords, chunk: UnloadedRegion) {}

    fn flush(&mut self) {}
}
