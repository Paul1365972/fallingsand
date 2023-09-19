use rustc_hash::FxHashSet;

use crate::{
    cell::{
        cached_cell::CachedSimulationCell,
        tile::MyTile,
    },
    entity::entity::MyEntity,
    util::coords::{WorldRegionCoords, CHUNKS_PER_REGION, TILES_PER_CHUNK},
};

#[repr(align(64))]
#[derive(Debug)]
pub struct TileChunk {
    pub tiles: [MyTile; TILES_PER_CHUNK * TILES_PER_CHUNK],
    //pub tile_entities: Vec<((u8, u8), TileEntity)>,
    pub bounds: (u8, u8, u8, u8),
    pub old_bounds: (u8, u8, u8, u8),
}

impl TileChunk {
    pub fn new(tiles: [MyTile; TILES_PER_CHUNK * TILES_PER_CHUNK]) -> Self {
        Self {
            tiles,
            //tile_entities: vec![],
            bounds: (0, 0, TILES_PER_CHUNK as u8, TILES_PER_CHUNK as u8),
            old_bounds: (0, 0, 0, 0),
        }
    }
}

pub struct Region {
    pub chunks: Box<[TileChunk; CHUNKS_PER_REGION * CHUNKS_PER_REGION]>,
    pub entity_keys: FxHashSet<EntityKey>,
    pub simulation_cells:
        Option<Box<[[CachedSimulationCell; CHUNKS_PER_REGION * CHUNKS_PER_REGION / 4 / 4]; 4]>>,
    pub num_neighbors: u8,
}

pub struct UnloadedRegion {
    pub tile_chunk: Box<[TileChunk; CHUNKS_PER_REGION * CHUNKS_PER_REGION]>,
    pub entities: Vec<MyEntity>,
}

impl Region {
    pub fn initalize_simulation_cells(mut neighbors: [&mut Region; 9]) {
        assert!(neighbors[4].num_neighbors == 8);
        let cells: [[CachedSimulationCell; CHUNKS_PER_REGION * CHUNKS_PER_REGION / 4 / 4]; 4] =
            std::array::from_fn(|offset_index| {
                let index_x = offset_index % 2;
                let index_y = offset_index / 2;
                std::array::from_fn(|full_cell_index| {
                    let full_cell_x = full_cell_index % (CHUNKS_PER_REGION / 4);
                    let full_cell_y = full_cell_index / (CHUNKS_PER_REGION / 4);
                    let pointers = std::array::from_fn(|chunk_index| {
                        let chunk_x = chunk_index % 4;
                        let chunk_y = chunk_index / 4;
                        let x = 7 + index_x * 2 + full_cell_x * 4 + chunk_x;
                        let y = 7 + index_y * 2 + full_cell_y * 4 + chunk_y;
                        println!("XY: {}, {}", x, y);
                        let chunk: *mut TileChunk = &mut neighbors[x / 8 + y / 8 * 3].chunks
                            [x % 8 + y % 8 * CHUNKS_PER_REGION];
                        chunk
                    });
                    CachedSimulationCell::new(pointers)
                })
            });
        neighbors[4].simulation_cells = Some(Box::new(cells));
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct EntityKey(u32);

pub struct EntityEntry {
    pub chunk_coords: WorldRegionCoords,
    pub entity: MyEntity,
}

impl EntityEntry {
    pub fn new(coords: WorldRegionCoords, entity: MyEntity) -> Self {
        Self {
            chunk_coords: coords,
            entity,
        }
    }
}
