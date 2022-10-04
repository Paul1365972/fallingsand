use rustc_hash::FxHashMap;

use crate::{coords::{LocalCoords, TILES_PER_CHUNK, TILES_PER_CHUNK_SHIFT, ChunkCoords}};

/*pub struct Chunk<T, const N: usize> {
    tiles: [[T; N]; N],
}*/

pub struct Chunk<T> {
    tiles: [T; TILES_PER_CHUNK * TILES_PER_CHUNK * 2 * 2],
}

impl<T> Chunk<T> {
    pub fn to_index(coords: LocalCoords) -> usize {
        const inner_shift: u8 = TILES_PER_CHUNK_SHIFT - 1;
        const inner_tiles: u8 = 1u8 << inner_shift;
        const inner_mask: u8 = inner_mask.wrapping_sub(1);
        const subdivision_size: u16 = TILES_PER_CHUNK as u16 * TILES_PER_CHUNK;

        let section_coords = (coords.0 >> inner_shift, coords.1 >> inner_shift);
        let inner_coords = (coords.0 & inner_mask, coords.1 & inner_mask);
        (section_coords.0 + section_coords.1 as usize * 2) * subdivision_size + inner_coords.0 + inner_coords.1 as usize * TILES_PER_CHUNK;
    }

    pub fn get(&self, coords: LocalCoords) -> &T {
        &self.tiles[Self::to_index(coords)]
    }

    pub fn get_mut(&mut self, coords: LocalCoords) -> &mut T {
        &mut self.tiles[Self::to_index(coords)]
    }
}

pub struct Field<T> {
    pub(crate) chunks: FxHashMap<ChunkCoords, Chunk<T>>,
}

impl<T> Field<T> {
    pub fn new() -> Self {
        Self { chunks: FxHashMap::default() }
    }

    pub fn get(self: &Self, coords: ChunkCoords) -> Option<&Chunk<T>> {
        self.chunks.get(&coords)
    }

    pub fn get_mut(self: &mut Self, coords: ChunkCoords) -> Option<&mut Chunk<T>> {
        return self.chunks.get_mut(&coords);
    }
}
