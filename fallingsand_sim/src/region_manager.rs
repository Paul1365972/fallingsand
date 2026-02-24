use crate::{
    cell::tile::{Tile, TileVariant},
    chunk::{TileChunk, UnloadedRegion},
    util::coords::{WorldRegionCoords, CHUNKS_PER_REGION},
};
use rustc_hash::FxHashMap;
use std::{fs, mem::ManuallyDrop};

pub struct ChunkManager {
    cache: FxHashMap<WorldRegionCoords, UnloadedRegion>,
}

impl ChunkManager {
    pub fn new() -> Self {
        let data = fs::read("./world/universe");
        let cache = match data {
            Ok(data) => bincode::deserialize(&data).unwrap(),
            Err(_) => Default::default(),
        };
        Self { cache }
    }

    pub fn load(&mut self, coords: &WorldRegionCoords) -> UnloadedRegion {
        self.cache
            .remove(coords)
            .unwrap_or_else(|| Self::generate_chunk(coords))
    }

    pub fn unload(&mut self, coords: &WorldRegionCoords, region: UnloadedRegion) {
        self.cache.insert(coords.clone(), region);
    }

    pub fn save(&mut self) {
        //TODO bincode::serialize_into(writer, value)
        let data = bincode::serialize(&self.cache).unwrap();
        fs::write("./world/universe", data).unwrap();
    }

    fn generate_chunk(coords: &WorldRegionCoords) -> UnloadedRegion {
        let mut chunks = Vec::with_capacity(CHUNKS_PER_REGION * CHUNKS_PER_REGION);
        for _ in 0..(CHUNKS_PER_REGION * CHUNKS_PER_REGION) {
            chunks.push(TileChunk::new(std::array::from_fn(|_| Tile {
                variant: TileVariant::AIR,
                ..Default::default()
            })));
        }
        let chunks = unsafe {
            Box::from_raw(ManuallyDrop::new(chunks).as_mut_ptr()
                as *mut [TileChunk; CHUNKS_PER_REGION * CHUNKS_PER_REGION])
        };
        UnloadedRegion {
            tile_chunk: chunks,
            entities: vec![],
        }
    }
}
