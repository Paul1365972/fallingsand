use std::ops::{Add, Sub};

pub const TILES_PER_CHUNK_SHIFT: u8 = 6;
pub const TILES_PER_CHUNK: u8 = 1u8 << TILES_PER_CHUNK_SHIFT;
pub const TILES_PER_CHUNK_MASK: u8 = TILES_PER_CHUNK.wrapping_sub(1);
pub const SPEED_OF_LIGHT: u8 = TILES_PER_CHUNK;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorldCoords {
    x: i32,
    y: i32,
}

impl WorldCoords {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn to_chunk_coords(self) -> ChunkCoords {
        ChunkCoords::new(
            self.x as u8 & TILES_PER_CHUNK_MASK,
            self.y as u8 & TILES_PER_CHUNK_MASK,
        )
    }

    pub fn to_world_chunk_coords(self) -> WorldChunkCoords {
        WorldChunkCoords::new(
            self.x >> TILES_PER_CHUNK_SHIFT,
            self.y >> TILES_PER_CHUNK_SHIFT,
        )
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorldChunkCoords {
    x: i32,
    y: i32,
}

impl WorldChunkCoords {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn to_world_coords(self, coords: ChunkCoords) -> WorldCoords {
        WorldCoords::new(
            (self.x << TILES_PER_CHUNK_SHIFT) | (coords.x & TILES_PER_CHUNK_MASK) as i32,
            (self.y << TILES_PER_CHUNK_SHIFT) | (coords.y & TILES_PER_CHUNK_MASK) as i32,
        )
    }

    pub fn to_world_cell_coords(self, offset: (i32, i32)) -> (i32, i32) {
        ((self.x - offset.0) >> 2, (self.y - offset.1) >> 2)
    }

    pub fn to_cell_chunk_index(self, offset: (i32, i32)) -> usize {
        let x = (self.x - offset.0) & 0x3;
        let y = (self.y - offset.1) & 0x3;
        x as usize + y as usize * 4
    }

    pub fn to_tuple(self) -> (i32, i32) {
        (self.x, self.y)
    }
}

impl Add<(i32, i32)> for &WorldChunkCoords {
    type Output = WorldChunkCoords;

    fn add(self, rhs: (i32, i32)) -> Self::Output {
        WorldChunkCoords::new(self.x + rhs.0, self.y + rhs.1)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChunkCoords {
    x: u8,
    y: u8,
}

impl ChunkCoords {
    pub fn new(x: u8, y: u8) -> Self {
        assert!(x < 64);
        assert!(y < 64);
        Self { x, y }
    }

    pub fn to_chunk_tile_index(self) -> usize {
        self.x as usize + self.y as usize * TILES_PER_CHUNK as usize
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellCoords {
    x: u8,
    y: u8,
}

impl CellCoords {
    pub fn new(x: u8, y: u8) -> Self {
        Self { x, y }
    }

    pub fn to_chunk_coords(self) -> ChunkCoords {
        ChunkCoords::new(self.x & TILES_PER_CHUNK_MASK, self.y & TILES_PER_CHUNK_MASK)
    }

    pub fn to_cell_chunk_index(self) -> usize {
        let x = (self.x >> TILES_PER_CHUNK_SHIFT) & 0x3;
        let y = (self.y >> TILES_PER_CHUNK_SHIFT) & 0x3;
        x as usize + y as usize * 4
    }

    pub fn above(self) -> CellCoords {
        self + CellCoords::new(0, 1)
    }

    pub fn below(self) -> CellCoords {
        self - CellCoords::new(0, 1)
    }

    pub fn left(self) -> CellCoords {
        self - CellCoords::new(1, 0)
    }

    pub fn right(self) -> CellCoords {
        self + CellCoords::new(1, 0)
    }
}

impl Add<CellCoords> for CellCoords {
    type Output = CellCoords;

    fn add(self, rhs: CellCoords) -> Self::Output {
        CellCoords::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub<CellCoords> for CellCoords {
    type Output = CellCoords;

    fn sub(self, rhs: CellCoords) -> Self::Output {
        CellCoords::new(self.x - rhs.x, self.y - rhs.y)
    }
}
