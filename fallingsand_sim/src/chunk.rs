use std::collections::HashSet;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::coords::{ChunkCoords, TILES_PER_CHUNK, WorldChunkCoords};

#[derive(Clone)]
pub struct TileChunk<T> {
    tiles: Box<[T; TILES_PER_CHUNK as usize * TILES_PER_CHUNK as usize]>,
}

impl<T> TileChunk<T> {
    pub fn new(tiles: [T; TILES_PER_CHUNK as usize * TILES_PER_CHUNK as usize]) -> Self {
        Self {
            tiles: Box::new(tiles),
        }
    }

    pub fn get(&self, coords: ChunkCoords) -> &T {
        &self.tiles[coords.to_chunk_tile_index()]
    }

    pub fn get_mut(&mut self, coords: ChunkCoords) -> &mut T {
        &mut self.tiles[coords.to_chunk_tile_index()]
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct EntityKey(u32);

pub struct EntityEntry<E> {
    pub chunk_coords: WorldChunkCoords,
    pub entity: E,
}

impl<E> EntityEntry<E> {
    pub fn new(chunk_coords: WorldChunkCoords, entity: E) -> Self { Self { chunk_coords, entity } }
}

#[derive(Default)]
pub struct EntityChunk {
    entities: FxHashSet<EntityKey>,
}

impl EntityChunk {
    pub fn new(entities: FxHashSet<EntityKey>) -> Self { Self { entities: FxHashSet::default() } }

    pub fn entities(&self) -> &FxHashSet<EntityKey> {
        &self.entities
    }

    pub fn entities_mut(&mut self) -> &mut FxHashSet<EntityKey> {
        &mut self.entities
    }
}
