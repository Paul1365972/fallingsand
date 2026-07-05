pub mod cell;
pub mod chunk;
pub mod coords;
pub mod material;
pub mod region;

pub const TICK_RATE: u32 = 60;
pub const TICK_DT: f32 = 1.0 / TICK_RATE as f32;

pub use cell::Cell;
pub use chunk::{CHUNK_AREA, CHUNK_SIZE, Chunk, DirtyRect};
pub use coords::{CellOffset, CellPos, ChunkOffset, ChunkPos, RegionPos};
pub use material::{Material, MaterialId, MaterialRegistry, Phase, Reaction, ReactionDef};
pub use region::{REGION_AREA_CHUNKS, REGION_SIZE_CELLS, REGION_SIZE_CHUNKS, Region};
