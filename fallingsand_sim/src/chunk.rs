use crate::coords::{ChunkCoords, TILES_PER_CHUNK};

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

#[derive(Default)]
pub struct EntityChunk<T> {
    entities: Vec<T>,
}

impl<T> EntityChunk<T> {
    pub fn entities(&self) -> &[T] {
        self.entities.as_ref()
    }

    pub fn entities_mod(&mut self) -> &mut [T] {
        &mut self.entities
    }

    pub fn entities_mut(&mut self) -> &mut Vec<T> {
        &mut self.entities
    }
}
