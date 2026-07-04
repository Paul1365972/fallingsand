use serde::{Deserialize, Serialize};

pub const CHUNK_BITS: u32 = 6;
pub const REGION_BITS: u32 = 3;

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct CellPos {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegionPos {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellOffset {
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkOffset {
    pub x: u8,
    pub y: u8,
}

impl CellPos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub const fn translated(self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x.wrapping_add(dx),
            y: self.y.wrapping_add(dy),
        }
    }

    pub const fn chunk(self) -> ChunkPos {
        ChunkPos {
            x: self.x >> CHUNK_BITS,
            y: self.y >> CHUNK_BITS,
        }
    }

    pub const fn offset(self) -> CellOffset {
        CellOffset {
            x: (self.x & ((1 << CHUNK_BITS) - 1)) as u8,
            y: (self.y & ((1 << CHUNK_BITS) - 1)) as u8,
        }
    }

    pub const fn region(self) -> RegionPos {
        self.chunk().region()
    }
}

impl ChunkPos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub const fn translated(self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x.wrapping_add(dx),
            y: self.y.wrapping_add(dy),
        }
    }

    pub const fn region(self) -> RegionPos {
        RegionPos {
            x: self.x >> REGION_BITS,
            y: self.y >> REGION_BITS,
        }
    }

    pub const fn offset(self) -> ChunkOffset {
        ChunkOffset {
            x: (self.x & ((1 << REGION_BITS) - 1)) as u8,
            y: (self.y & ((1 << REGION_BITS) - 1)) as u8,
        }
    }

    pub const fn base_cell(self) -> CellPos {
        CellPos {
            x: self.x << CHUNK_BITS,
            y: self.y << CHUNK_BITS,
        }
    }

    pub const fn cell(self, offset: CellOffset) -> CellPos {
        CellPos {
            x: (self.x << CHUNK_BITS) | offset.x as i32,
            y: (self.y << CHUNK_BITS) | offset.y as i32,
        }
    }
}

impl RegionPos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub const fn base_chunk(self) -> ChunkPos {
        ChunkPos {
            x: self.x << REGION_BITS,
            y: self.y << REGION_BITS,
        }
    }

    pub const fn chunk(self, offset: ChunkOffset) -> ChunkPos {
        ChunkPos {
            x: (self.x << REGION_BITS) | offset.x as i32,
            y: (self.y << REGION_BITS) | offset.y as i32,
        }
    }

    pub const fn zorder_key(self) -> u64 {
        interleave(self.x as u32) | (interleave(self.y as u32) << 1)
    }

    pub const fn from_zorder_key(key: u64) -> Self {
        Self {
            x: deinterleave(key) as i32,
            y: deinterleave(key >> 1) as i32,
        }
    }
}

const fn interleave(v: u32) -> u64 {
    let mut x = v as u64;
    x = (x | (x << 16)) & 0x0000_FFFF_0000_FFFF;
    x = (x | (x << 8)) & 0x00FF_00FF_00FF_00FF;
    x = (x | (x << 4)) & 0x0F0F_0F0F_0F0F_0F0F;
    x = (x | (x << 2)) & 0x3333_3333_3333_3333;
    x = (x | (x << 1)) & 0x5555_5555_5555_5555;
    x
}

const fn deinterleave(v: u64) -> u32 {
    let mut x = v & 0x5555_5555_5555_5555;
    x = (x | (x >> 1)) & 0x3333_3333_3333_3333;
    x = (x | (x >> 2)) & 0x0F0F_0F0F_0F0F_0F0F;
    x = (x | (x >> 4)) & 0x00FF_00FF_00FF_00FF;
    x = (x | (x >> 8)) & 0x0000_FFFF_0000_FFFF;
    x = (x | (x >> 16)) & 0x0000_0000_FFFF_FFFF;
    x as u32
}

impl CellOffset {
    pub const fn new(x: u8, y: u8) -> Self {
        debug_assert!(x < 64 && y < 64);
        Self { x, y }
    }

    pub const fn index(self) -> usize {
        ((self.y as usize) << CHUNK_BITS) | self.x as usize
    }

    pub const fn from_index(index: usize) -> Self {
        Self {
            x: (index & ((1 << CHUNK_BITS) - 1)) as u8,
            y: (index >> CHUNK_BITS) as u8,
        }
    }
}

impl ChunkOffset {
    pub const fn new(x: u8, y: u8) -> Self {
        debug_assert!(x < 8 && y < 8);
        Self { x, y }
    }

    pub const fn index(self) -> usize {
        ((self.y as usize) << REGION_BITS) | self.x as usize
    }

    pub const fn from_index(index: usize) -> Self {
        Self {
            x: (index & ((1 << REGION_BITS) - 1)) as u8,
            y: (index >> REGION_BITS) as u8,
        }
    }
}
