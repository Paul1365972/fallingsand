pub const TILES_PER_CHUNK_SHIFT: u8 = 7;
pub const TILES_PER_CHUNK: u8 = 1u8 << TILES_PER_CHUNK_SHIFT;
pub const TILES_PER_CHUNK_MASK: u8 = TILES_PER_CHUNK.wrapping_sub(1);
pub const SPEED_OF_LIGHT: u8 = TILES_PER_CHUNK;


#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct WorldCoords {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ChunkCoords {
    x: i32,
    y: i32,
}

impl WorldCoords {
    fn to_local_coords(self) -> LocalCoords {
        (self.x as u8 & TILES_PER_CHUNK_MASK, self.y as u8 & TILES_PER_CHUNK_MASK)
    }

    fn to_chunk_coords(self) -> ChunkCoords {
        ChunkCoords { x: self.x >> TILES_PER_CHUNK_SHIFT, y: self.y >> TILES_PER_CHUNK_SHIFT }
    }
}

impl ChunkCoords {
    fn to_world_coords(self, coords: LocalCoords) -> WorldCoords {
        WorldCoords { x: (self.x << TILES_PER_CHUNK_SHIFT) | (coords.0 & TILES_PER_CHUNK_MASK), y: (self.y << TILES_PER_CHUNK_SHIFT) | (coords.1 & TILES_PER_CHUNK_MASK) }
    }
}

pub type LocalCoords = (u8, u8);

pub trait LocalCoordsConvertable {
    fn to_subdivision_coords(self) -> (u8, u8);
}

impl LocalCoordsConvertable for LocalCoords {
    fn to_subdivision_coords(self) -> (u8, u8) {
        (self.0 & TILES_PER_CHUNK >> TILES_PER_CHUNK_SHIFT, self.1 & TILES_PER_CHUNK >> TILES_PER_CHUNK_SHIFT)
    }
}
