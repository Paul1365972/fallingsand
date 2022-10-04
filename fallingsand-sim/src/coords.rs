pub const TILES_PER_CHUNK_SHIFT: u8 = 6;
pub const TILES_PER_CHUNK: u8 = 1 << TILES_PER_CHUNK_SHIFT;
pub const TILES_PER_CHUNK_MASK: u8 = (1u8 << TILES_PER_CHUNK_SHIFT).wrapping_sub(1);
pub const SPEED_OF_LIGHT: u8 = TILES_PER_CHUNK;

pub const CHUNKS_PER_SECTION_SHIFT: u8 = 1;
pub const CHUNKS_PER_SECTION: u8 = 1 < CHUNKS_PER_SECTION_SHIFT;
pub const TILES_PER_SECTION_SHIFT: u8 = TILES_PER_CHUNK_SHIFT + CHUNKS_PER_SECTION_SHIFT;
pub const TILES_PER_SECTION: u8 = 1 << TILES_PER_SECTION_SHIFT;
pub const TILES_PER_SECTION_MASK: u8 = (1u8 << TILES_PER_SECTION_SHIFT).wrapping_sub(1);


#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct WorldCoords {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct SectionCoords {
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
        (self.x as u8 & TILES_PER_SECTION, self.y as u8 & TILES_PER_SECTION)
    }

    fn to_section_coords(self) -> SectionCoords {
        SectionCoords { x: self.x >> TILES_PER_SECTION_SHIFT, y: self.y >> TILES_PER_SECTION_SHIFT }
    }

    fn to_chunk_coords(self) -> ChunkCoords {
        ChunkCoords { x: self.x >> TILES_PER_CHUNK_SHIFT, y: self.y >> TILES_PER_CHUNK_SHIFT }
    }
}

impl ChunkCoords {
    fn to_section_coords(self) -> SectionCoords {
        SectionCoords { x: self.x >> CHUNKS_PER_SECTION_SHIFT, y: self.y >> CHUNKS_PER_SECTION_SHIFT }
    }
}

pub type LocalCoords = (u8, u8);

pub trait LocalCoordsConvertable {
    fn to_section_coords(self) -> LocalCoords;
}

impl LocalCoordsConvertable for LocalCoords {
    fn to_section_coords(self) -> LocalCoords {
        (self.0 & TILES_PER_CHUNK >> TILES_PER_CHUNK_SHIFT, self.1 & TILES_PER_CHUNK >> TILES_PER_CHUNK_SHIFT)
    }
}
