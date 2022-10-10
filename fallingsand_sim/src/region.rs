use rustc_hash::FxHashMap;

use crate::{
    aabb::AABB,
    chunk::{EntityChunk, TileChunk, EntityKey, EntityEntry},
    coords::WorldChunkCoords,
};

pub struct DisjointRegion<T, E> {
    bounds: AABB,
    chunks: FxHashMap<WorldChunkCoords, Chunk<T>>,
    entities: FxHashMap<EntityKey, (WorldChunkCoords, E)>,
}

pub struct Chunk<T> {
    tile_chunk: TileChunk<T>,
    entity_chunk: EntityChunk,
}

impl<T> Chunk<T> {
    pub fn new(tile_chunk: TileChunk<T>, entity_chunk: EntityChunk) -> Self {
        Self {
            tile_chunk,
            entity_chunk,
        }
    }

    pub fn tile_chunk(&self) -> &TileChunk<T> {
        &self.tile_chunk
    }

    pub fn tile_chunk_mut(&mut self) -> &mut TileChunk<T> {
        &mut self.tile_chunk
    }

    pub fn entity_chunk(&self) -> &EntityChunk {
        &self.entity_chunk
    }

    pub fn entity_chunk_mut(&mut self) -> &mut EntityChunk {
        &mut self.entity_chunk
    }
}

impl<T, E> DisjointRegion<T, E> {
    pub fn new_unchecked() -> Self {
        Self {
            bounds: AABB::from_point((123, 456)),
            chunks: FxHashMap::default(),
            entities: FxHashMap::default(),
        }
    }

    pub fn new2(coords: WorldChunkCoords, chunk: Chunk<T>) -> Self {
        let mut chunks = FxHashMap::default();
        chunks.insert(coords, chunk);
        Self {
            bounds: AABB::from_point(coords.to_tuple()),
            chunks,
            entities: FxHashMap::default(),
        }
    }

    pub fn merge(&mut self, other: DisjointRegion<T, E>) {
        self.bounds = self.bounds.union(&other.bounds);
        self.chunks.extend(other.chunks.into_iter());
    }

    pub fn chunks_iter(&self) -> std::collections::hash_map::Iter<WorldChunkCoords, Chunk<T>> {
        self.chunks.iter()
    }

    pub fn contains_chunk(&self, coords: &WorldChunkCoords) -> bool {
        self.chunks.contains_key(coords)
    }

    pub fn chunks_iter_mut(
        &mut self,
    ) -> std::collections::hash_map::IterMut<WorldChunkCoords, Chunk<T>> {
        self.chunks.iter_mut()
    }

    pub fn insert(&mut self, coords: WorldChunkCoords, chunk: Chunk<T>) {
        self.chunks.insert(coords, chunk);
    }

    pub fn remove(&mut self, coords: WorldChunkCoords) -> Option<Chunk<T>> {
        self.chunks.remove(&coords)
    }

    pub fn get(&self, coords: WorldChunkCoords) -> Option<&Chunk<T>> {
        self.chunks.get(&coords)
    }

    pub fn get_mut(&mut self, coords: WorldChunkCoords) -> Option<&mut Chunk<T>> {
        return self.chunks.get_mut(&coords);
    }

    pub fn get_entity(&self, key: EntityKey) -> Option<&EntityEntry<E>> {
        self.entities.get(&key)
    }

    pub fn entities(&self) -> &FxHashMap<EntityKey, EntityEntry<E>> {
        &self.entities
    }

    pub fn entities_mut(&mut self) -> &mut FxHashMap<EntityKey, EntityEntry<E>> {
        &mut self.entities
    }

    pub fn retain_entities<F>(&mut self, predicate: F) where
    F: FnMut(&EntityKey, &mut EntityEntry<E>) -> bool,{
        self.entities.retain(predicate);
    }
}
