use crate::{
    chunk::TileChunk,
    coords::{ChunkCoords, TILES_PER_CHUNK},
};

use self::tile::{Tile, Variant};

pub mod tile;
pub mod tilesimulator;

impl TileChunk<Tile> {
    pub fn new_air() -> Self {
        Self::new(
            [Tile {
                variant: Variant::AIR,
                ..Default::default()
            }; TILES_PER_CHUNK as usize * TILES_PER_CHUNK as usize],
        )
    }
}

impl TileChunk<Tile> {
    pub fn new_air_sand_mix() -> Self {
        let mut chunk = Self::new(
            [Tile {
                variant: Variant::AIR,
                ..Default::default()
            }; TILES_PER_CHUNK as usize * TILES_PER_CHUNK as usize],
        );
        for y in 0..TILES_PER_CHUNK {
            for x in 0..TILES_PER_CHUNK {
                if (x + y) & 4 == 0 {
                    chunk.get_mut(ChunkCoords::new(x, y)).variant = Variant::SAND;
                }
            }
        }
        chunk
    }
}
