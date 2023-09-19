use std::ops::{Add, Sub};

pub const TILES_PER_CHUNK_SHIFT: usize = 6;
pub const TILES_PER_CHUNK: usize = 1 << TILES_PER_CHUNK_SHIFT;
pub const TILES_PER_CHUNK_MASK: usize = TILES_PER_CHUNK - 1;

pub const CHUNKS_PER_REGION_SHIFT: usize = 3;
pub const CHUNKS_PER_REGION: usize = 1 << CHUNKS_PER_REGION_SHIFT;
pub const CHUNKS_PER_REGION_MASK: usize = CHUNKS_PER_REGION - 1;

pub const TILES_PER_REGION_SHIFT: usize = TILES_PER_CHUNK_SHIFT + CHUNKS_PER_REGION_SHIFT;
pub const TILES_PER_REGION: usize = 1 << TILES_PER_REGION_SHIFT;
pub const TILES_PER_REGION_MASK: usize = TILES_PER_REGION - 1;

pub const SPEED_OF_LIGHT: usize = TILES_PER_CHUNK;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorldCoords {
    x: i32,
    y: i32,
}

impl WorldCoords {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn to_world_region_coords(self) -> WorldRegionCoords {
        WorldRegionCoords::new(
            self.x >> TILES_PER_REGION_SHIFT,
            self.y >> TILES_PER_REGION_SHIFT,
        )
    }

    pub fn to_region_coords(&self) -> RegionCoords {
        RegionCoords::new(
            self.x as u16 & TILES_PER_REGION_MASK as u16,
            self.y as u16 & TILES_PER_REGION_MASK as u16,
        )
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorldRegionCoords {
    x: i32,
    y: i32,
}

impl WorldRegionCoords {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    //pub fn to_world_cell_coords(self, offset: (i32, i32)) -> (i32, i32) {
    //    ((self.x - offset.0) >> 2, (self.y - offset.1) >> 2)
    //}

    //pub fn to_cell_chunk_index(self, offset: (i32, i32)) -> usize {
    //    let x = (self.x - offset.0) & 0x3;
    //    let y = (self.y - offset.1) & 0x3;
    //    x as usize + y as usize * 4
    //}

    pub fn neighbors_exclusive(&self) -> [WorldRegionCoords; 8] {
        [
            self + (-1, -1),
            self + (0, -1),
            self + (1, -1),
            self + (-1, 0),
            self + (1, 0),
            self + (-1, 1),
            self + (0, 1),
            self + (1, 1),
        ]
    }

    pub fn neighbors_inclusive(&self) -> [WorldRegionCoords; 9] {
        [
            self + (-1, -1),
            self + (0, -1),
            self + (1, -1),
            self + (-1, 0),
            self + (0, 0),
            self + (1, 0),
            self + (-1, 1),
            self + (0, 1),
            self + (1, 1),
        ]
    }
}

impl Add<(i32, i32)> for &WorldRegionCoords {
    type Output = WorldRegionCoords;

    fn add(self, rhs: (i32, i32)) -> Self::Output {
        WorldRegionCoords::new(self.x + rhs.0, self.y + rhs.1)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegionCoords {
    x: u16,
    y: u16,
}

impl RegionCoords {
    pub fn new(x: u16, y: u16) -> Self {
        assert!((x as usize) < TILES_PER_REGION);
        assert!((y as usize) < TILES_PER_REGION);
        Self { x, y }
    }

    pub fn to_chunk_coords(&self) -> ChunkCoords {
        ChunkCoords::new(
            self.x as u8 & TILES_PER_CHUNK_MASK as u8,
            self.y as u8 & TILES_PER_CHUNK_MASK as u8,
        )
    }

    pub fn to_chunk_index(&self) -> usize {
        (self.x as usize >> TILES_PER_CHUNK_SHIFT)
            + ((self.y as usize >> TILES_PER_CHUNK_SHIFT) << CHUNKS_PER_REGION_SHIFT)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChunkCoords {
    x: u8,
    y: u8,
}

impl ChunkCoords {
    pub fn new(x: u8, y: u8) -> Self {
        assert!((x as usize) < TILES_PER_CHUNK);
        assert!((y as usize) < TILES_PER_CHUNK);
        Self { x, y }
    }

    pub fn to_tile_index(&self) -> usize {
        (self.x as usize) + ((self.y as usize) << TILES_PER_CHUNK_SHIFT)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellCoords {
    x: u16,
    y: u16,
}

impl CellCoords {
    pub fn new(x: u16, y: u16) -> Self {
        assert!((x as usize) < TILES_PER_CHUNK * 4);
        assert!((y as usize) < TILES_PER_CHUNK * 4);
        Self { x, y }
    }

    pub fn to_chunk_index(self) -> usize {
        let x = self.x as usize & TILES_PER_CHUNK_MASK;
        let y = self.y as usize & TILES_PER_CHUNK_MASK;
        x + y * TILES_PER_CHUNK
    }

    pub fn to_cell_index(self) -> usize {
        let x = (self.x as usize >> TILES_PER_CHUNK_SHIFT) & 0x3;
        let y = (self.y as usize >> TILES_PER_CHUNK_SHIFT) & 0x3;
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
