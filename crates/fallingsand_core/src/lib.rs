pub mod calendar;
pub mod celestial;
pub mod cell;
pub mod chunk;
pub mod content;
pub mod coords;
pub mod fixed;
pub mod item;
pub mod material;
pub mod region;

pub use material::TICK_RATE;
pub const TICK_DT: f32 = 1.0 / TICK_RATE as f32;

pub const fn ticks_from_secs(secs: f32) -> u64 {
    (secs * TICK_RATE as f32 + 0.5) as u64
}

pub const GRID_GRAVITY: f32 = 600.0;
pub const MAX_HP: f32 = 100.0;
pub const MAX_AIR_SECS: f32 = 12.0;
pub const REACH: f32 = 100.0;
pub const SURVIVAL_REACH: f32 = 20.0;

pub use calendar::{Calendar, DAY_UNITS, SEASON_DAYS, Season};
pub use celestial::{CelestialState, smoothstep};
pub use cell::{Cell, VEL_ONE};
pub use chunk::{CHUNK_AREA, CHUNK_SIZE, Chunk, DirtyRect};
pub use coords::{
    CARDINAL_NEIGHBORS, CellOffset, CellPos, ChunkOffset, ChunkPos, RegionPos, ray_cells,
};
pub use fixed::Fixed;
pub use item::{
    HOTBAR_SLOTS, Inventory, ItemId, ItemInfo, ItemStack, MAIN_SLOTS, PLAYER_SLOTS, Recipe,
    ToolSpec,
};
pub use material::{
    Burning, BurningKind, Dynamics, GasDynamics, Ignition, LiquidDynamics, MaterialId,
    MaterialInfo, Phase, PowderDynamics, Reaction, Scale, SealedBurn, Tag, Tags,
};
pub use region::{REGION_AREA_CHUNKS, REGION_SIZE_CELLS, REGION_SIZE_CHUNKS, Region};
