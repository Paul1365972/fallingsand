use std::net::TcpStream;

use rustc_hash::FxHashSet;

use crate::{chunk::Region, util::coords::WorldRegionCoords, world::World};

pub struct Connection {
    stream: TcpStream,
}

pub struct Orchestrator {
    world: World,
}

pub struct ViewReceiver {
    id: u32, // ???
    coords: WorldRegionCoords,
    loaded_chunks: FxHashSet<WorldRegionCoords>,
}

impl ViewReceiver {
    fn update_view(
        &mut self,
        world: &World,
        // updated_chunks: Option<FxHashSet<WorldChunkCoords>>,
    ) {
        let region = world;
        // let updated_chunks = updated_chunks.unwrap_or_else(|| {
        // let mut set = receiver.loaded_chunks.clone();
        // set.extend(region.chunks_iter().map(|(&k, v)| k));
        // set
        // });

        // let to_load = region
        //     .chunks_iter()
        //     .filter(|(x, _)| !self.loaded_chunks.contains(x))
        //     .collect_vec();
        // let to_unload = self
        //     .loaded_chunks
        //     .iter()
        //     .filter(|x| !region.contains_chunk(x))
        //     .collect_vec();
        // self.loaded_chunks.clear();
        // self.loaded_chunks
        //     .extend(region.chunks_iter().map(|(x, _)| x));
    }
}
