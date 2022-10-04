use rustc_hash::FxHashMap;

use crate::{coords::{LocalCoords, SectionCoords, LocalCoordsConvertable, TILES_PER_CHUNK}};

/*pub struct Chunk<T, const N: usize> {
    tiles: [[T; N]; N],
}*/

pub struct Chunk<T> {
    tiles: [T; TILES_PER_CHUNK * TILES_PER_CHUNK],
}

impl<T> Chunk<T> {
    const MASK: u8 = (1 << TILES_PER_CHUNK) - 1;

    pub fn to_index(coords: LocalCoords) -> usize {
        (coords.0 & Self::MASK) as usize + (coords.1 & Self::MASK) as usize * TILES_PER_CHUNK
    }

    pub fn get(&self, coords: LocalCoords) -> &T {
        &self.tiles[Self::to_index(coords)]
    }

    pub fn get_mut(&mut self, coords: LocalCoords) -> &mut T {
        &mut self.tiles[Self::to_index(coords)]
    }
}

pub struct Section<T> {
    chunks: [Chunk<T>; 4],
}

impl<T> Section<T> {
    pub fn to_index(coords: LocalCoords) -> usize {
        let section_coords = coords.to_section_coords();
        (section_coords.0 & Self::MASK) as usize + (section_coords.1 & Self::MASK) as usize * TILES_PER_CHUNK
    }

    pub fn get_chunk(&self, coords: LocalCoords) -> &Chunk<T> {
        &self.chunks[Self::to_index(coords)]
    }

    pub fn get(&self, coords: LocalCoords) -> &T {
        self.get_chunk(coords).get(coords)
    }

    pub fn get_chunk_mut(&self, coords: LocalCoords) -> &mut Chunk<T> {
        &mut self.chunks[Self::to_index(coords)]
    }

    pub fn get_mut(&mut self, coords: LocalCoords) -> &mut T {
        self.get_chunk_mut(coords).get_mut(coords)
    }
}

pub struct Field<T> {
    pub(crate) sections: FxHashMap<SectionCoords, Section<T>>,
}

impl<T> Field<T> {
    pub fn new() -> Self {
        Self { sections: FxHashMap::default() }
    }

    pub fn get(self: &Self, coords: SectionCoords) -> Option<&Section<T>> {
        self.sections.get(&coords)
    }

    pub fn get_mut(self: &mut Self, coords: SectionCoords) -> Option<&mut Section<T>> {
        return self.sections.get_mut(&coords);
    }
}
