use rustc_hash::FxHashMap;

use crate::{
    cell::substep::TileSimulationSubStep,
    chunk::{Chunk, EntityEntry, EntityKey, EntityKeyChunk, TileChunk},
    entity::{chunk_ticket::ChunkTicketKey, entity::MyEntityVariant},
    util::{aabb::AABB, coords::WorldChunkCoords},
    world::GlobalContext,
};

pub struct DisjointRegion {
    bounds: AABB,
    chunks: FxHashMap<WorldChunkCoords, Chunk>,
    entities: FxHashMap<EntityKey, EntityEntry>,
}

impl DisjointRegion {
    pub fn new_unchecked() -> Self {
        Self {
            bounds: AABB::from_point((123, 456)),
            chunks: FxHashMap::default(),
            entities: FxHashMap::default(),
        }
    }

    pub fn merge(&mut self, other: DisjointRegion) {
        todo!()
    }

    pub fn chunks_iter(&self) -> std::collections::hash_map::Iter<WorldChunkCoords, Chunk> {
        self.chunks.iter()
    }

    pub fn for_chunk_coords<'a, F>(&self, f: F)
    where
        F: FnMut(&WorldChunkCoords),
    {
        self.chunks.keys().for_each(f);
    }

    pub fn for_entities_mut<'a, F>(&mut self, mut f: F)
    where
        F: FnMut(&EntityKey, &mut EntityEntry),
    {
        self.entities.iter_mut().for_each(|(k, v)| f(k, v));
    }

    pub fn contains_chunk(&self, coords: &WorldChunkCoords) -> bool {
        self.chunks.contains_key(coords)
    }

    pub fn for_tile_chunk_mut<'a, F>(&'a mut self, mut f: F)
    where
        F: FnMut(&WorldChunkCoords, &'a mut TileChunk),
    {
        self.chunks
            .iter_mut()
            .for_each(|(k, v)| f(k, v.tile_chunk_mut()));
    }

    pub fn insert_tile_chunk(&mut self, coords: WorldChunkCoords, tile_chunk: TileChunk) {
        self.chunks
            .insert(coords, Chunk::new(tile_chunk, EntityKeyChunk::default()));
    }

    pub fn unsafe_remove_tile_chunk(&mut self, coords: WorldChunkCoords) -> Option<TileChunk> {
        let chunk = self.chunks.remove(&coords);
        if let Some(chunk) = chunk {
            for key in chunk.entity_chunk().entities() {
                self.entities.remove(key);
            }
            return Some(chunk.into_tile_chunk());
        }
        None
    }

    pub fn unsafe_get(&self, coords: WorldChunkCoords) -> Option<&Chunk> {
        self.chunks.get(&coords)
    }

    pub fn unsafe_get_mut(&mut self, coords: WorldChunkCoords) -> Option<&mut Chunk> {
        return self.chunks.get_mut(&coords);
    }
}

impl DisjointRegion {
    pub fn step_tiles(&mut self, ctx: &GlobalContext) {
        for offset in [(0, 0), (0, 2), (2, 0), (2, 2)] {
            let mut substep = TileSimulationSubStep::new(self, offset);
            substep.step_tiles(ctx);
        }
        // To do maybe collect tile events here and apply them
    }
}

pub struct DisjointRegionTileAccessor<'a> {
    chunks: &'a mut FxHashMap<WorldChunkCoords, Chunk>,
}

impl<'a> DisjointRegionTileAccessor<'a> {
    pub fn get_chunk(&self, coords: WorldChunkCoords) -> Option<&Chunk> {
        self.chunks.get(&coords)
    }

    pub fn get_chunk_mut(&mut self, coords: WorldChunkCoords) -> Option<&mut Chunk> {
        self.chunks.get_mut(&coords)
    }
}

pub enum EntityStepResult {
    ChunkTicketMoved(ChunkTicketKey, (i32, i32)),
    ChunkTicketRemoved(ChunkTicketKey),
}

impl DisjointRegion {
    pub fn step_entities(&mut self) -> Vec<EntityStepResult> {
        let mut events = Vec::new();
        // Do step entity stuff here

        for (key, value) in self.entities.iter_mut() {
            value.entity.step(DisjointRegionTileAccessor {
                chunks: &mut self.chunks,
            });
        }

        // Remove entities
        self.entities.retain(|key, value| {
            let entity = &mut value.entity;
            if entity.should_remove() {
                self.chunks
                    .get_mut(&value.chunk_coords)
                    .unwrap()
                    .entity_chunk_mut()
                    .entities_mut()
                    .remove(key);
                if let MyEntityVariant::Player(Some(ticket_key)) = &entity.variant {
                    events.push(EntityStepResult::ChunkTicketRemoved(ticket_key.clone()));
                }
                return false;
            }
            true
        });

        // Move entities
        for (key, value) in self.entities.iter_mut() {
            let entity = &mut value.entity;
            let offset = entity.apply_move();
            if offset != (0, 0) {
                self.chunks
                    .get_mut(&value.chunk_coords)
                    .unwrap()
                    .entity_chunk_mut()
                    .entities_mut()
                    .remove(&key);
                let new_coords = &value.chunk_coords + offset;
                self.chunks
                    .get_mut(&new_coords)
                    .unwrap()
                    .entity_chunk_mut()
                    .entities_mut()
                    .insert(*key);

                if let MyEntityVariant::Player(Some(ticket_key)) = &entity.variant {
                    events.push(EntityStepResult::ChunkTicketMoved(
                        ticket_key.clone(),
                        offset,
                    ));
                }
            }
        }
        events
    }
}
