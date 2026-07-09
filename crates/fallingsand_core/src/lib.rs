pub mod calendar;
pub mod celestial;
pub mod cell;
pub mod chunk;
pub mod coords;
pub mod fixed;
pub mod item;
pub mod material;
pub mod region;

pub const TICK_RATE: u32 = 60;
pub const TICK_DT: f32 = 1.0 / TICK_RATE as f32;

pub const fn ticks_from_secs(secs: f32) -> u64 {
    (secs * TICK_RATE as f32 + 0.5) as u64
}
pub const GRID_GRAVITY: f32 = 600.0;
pub const MAX_AIR_SECS: f32 = 12.0;
pub const MOON_PHASES: u32 = 8;
pub const REACH: f32 = 80.0;
pub const SURVIVAL_REACH: f32 = 48.0;
pub const BRUSH_RADIUS: i32 = 3;

pub use calendar::{AGE_PER_SEC, AGE_PER_TICK, Calendar, DAY_UNITS, DRACONIC_UNITS, SYNODIC_UNITS};
pub use celestial::{CelestialState, smoothstep};
pub use cell::{Cell, VEL_ONE};
pub use chunk::{CHUNK_AREA, CHUNK_SIZE, Chunk, DirtyRect};
pub use coords::{CellOffset, CellPos, ChunkOffset, ChunkPos, RegionPos};
pub use fixed::Fixed;
pub use item::{
    HOTBAR_SLOTS, IconSpec, Inventory, ItemDef, ItemId, ItemRegistry, ItemStack, MAIN_SLOTS,
    MATERIAL_STACK_MAX, PLAYER_SLOTS, Recipe, RecipeRegistry,
};
pub use material::{
    Dynamics, Material, MaterialId, MaterialRegistry, Phase, Product, Reaction, ReactionDef,
    per_tick_chance, per_tick_keep,
};
pub use region::{REGION_AREA_CHUNKS, REGION_SIZE_CELLS, REGION_SIZE_CHUNKS, Region};
