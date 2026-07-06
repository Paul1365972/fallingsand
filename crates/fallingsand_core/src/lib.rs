pub mod cell;
pub mod chunk;
pub mod coords;
pub mod fixed;
pub mod material;
pub mod region;

pub const TICK_RATE: u32 = 60;
pub const TICK_DT: f32 = 1.0 / TICK_RATE as f32;
pub const DAY_SECS: f32 = 300.0;
pub const MAX_AIR_SECS: f32 = 12.0;
pub const MOON_PHASES: u32 = 8;
pub const REACH: f32 = 80.0;
pub const SURVIVAL_REACH: f32 = 48.0;
pub const BRUSH_RADIUS: i32 = 3;

pub use cell::Cell;
pub use chunk::{CHUNK_AREA, CHUNK_SIZE, Chunk, DirtyRect};
pub use coords::{CellOffset, CellPos, ChunkOffset, ChunkPos, RegionPos};
pub use fixed::Fixed;
pub use material::{Material, MaterialId, MaterialRegistry, Phase, Reaction, ReactionDef};
pub use region::{REGION_AREA_CHUNKS, REGION_SIZE_CELLS, REGION_SIZE_CHUNKS, Region};
