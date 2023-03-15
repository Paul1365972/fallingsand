use rustc_hash::FxHashSet;

use crate::{
    cell::tile::MyTile,
    entity::entity::MyEntity,
    util::coords::{ChunkCoords, WorldChunkCoords, TILES_PER_CHUNK},
};

#[derive(Clone)]
pub struct TileChunk {
    tiles: Box<[MyTile; TILES_PER_CHUNK as usize * TILES_PER_CHUNK as usize]>,
}

impl TileChunk {
    pub fn new(tiles: [MyTile; TILES_PER_CHUNK as usize * TILES_PER_CHUNK as usize]) -> Self {
        Self {
            tiles: Box::new(tiles),
        }
    }

    pub fn get(&self, coords: ChunkCoords) -> &MyTile {
        &self.tiles[coords.to_chunk_tile_index()]
    }

    pub fn get_mut(&mut self, coords: ChunkCoords) -> &mut MyTile {
        &mut self.tiles[coords.to_chunk_tile_index()]
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct EntityKey(u32);

pub struct EntityEntry {
    pub chunk_coords: WorldChunkCoords,
    pub entity: MyEntity,
}

impl EntityEntry {
    pub fn new(chunk_coords: WorldChunkCoords, entity: MyEntity) -> Self {
        Self {
            chunk_coords,
            entity,
        }
    }
}

#[derive(Default)]
pub struct EntityKeyChunk {
    entities: FxHashSet<EntityKey>,
}

impl EntityKeyChunk {
    pub fn new(entities: FxHashSet<EntityKey>) -> Self {
        Self { entities: entities }
    }

    pub fn entities(&self) -> &FxHashSet<EntityKey> {
        &self.entities
    }

    pub fn entities_mut(&mut self) -> &mut FxHashSet<EntityKey> {
        &mut self.entities
    }
}

pub struct Chunk {
    tile_chunk: TileChunk,
    entity_key_chunk: EntityKeyChunk,
}

impl Chunk {
    pub fn new(tile_chunk: TileChunk, entity_chunk: EntityKeyChunk) -> Self {
        Self {
            tile_chunk,
            entity_key_chunk: entity_chunk,
        }
    }

    pub fn tile_chunk(&self) -> &TileChunk {
        &self.tile_chunk
    }

    pub fn tile_chunk_mut(&mut self) -> &mut TileChunk {
        &mut self.tile_chunk
    }

    pub fn into_tile_chunk(self) -> TileChunk {
        self.tile_chunk
    }

    pub fn entity_chunk(&self) -> &EntityKeyChunk {
        &self.entity_key_chunk
    }

    pub fn entity_chunk_mut(&mut self) -> &mut EntityKeyChunk {
        &mut self.entity_key_chunk
    }
}
