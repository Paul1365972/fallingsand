use serde::{Deserialize, Serialize};

pub const TICK_RATE: u32 = 60;
pub const VEL_ONE: i32 = 1024;
pub const CHUNK_SIZE: usize = 64;
pub const CHUNK_AREA: usize = CHUNK_SIZE * CHUNK_SIZE;

pub const RANDOM_TICKS_PER_CHUNK: u32 = 4;

pub type PerSecond = f32;
pub type Seconds = f32;
pub type Fraction = f32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialId(pub u16);

impl MaterialId {
    pub const AIR: Self = Self(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    Empty,
    Solid,
    Powder,
    Liquid,
    Gas,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tag {
    Dissolvable,
    Hot,
    Player,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tags(u32);

impl Tags {
    pub const EMPTY: Self = Self(0);

    pub const fn new(tags: &[Tag]) -> Self {
        let mut bits = 0u32;
        let mut i = 0;
        while i < tags.len() {
            bits |= 1u32 << tags[i] as u32;
            i += 1;
        }
        Self(bits)
    }

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn contains(self, tag: Tag) -> bool {
        self.0 & (1u32 << tag as u32) != 0
    }

    #[inline]
    pub const fn union(self, other: Tags) -> Tags {
        Tags(self.0 | other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reaction {
    pub becomes: MaterialId,
    pub other_becomes: MaterialId,
    pub threshold: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ignition {
    pub into: MaterialId,
    pub open: u64,
    pub sealed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurningKind {
    Flame,
    Fuel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealedBurn {
    Becomes(MaterialId),
    Smoulder(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Burning {
    pub burn: u64,
    pub sealed: SealedBurn,
    pub emit: u64,
    pub residue: Option<(u64, MaterialId)>,
    pub burnout: MaterialId,
    pub kind: BurningKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scale(pub u32);

impl Scale {
    pub const ZERO: Scale = Scale(0);

    pub const fn apply(self, v: i32) -> i32 {
        let product = v as i64 * self.0 as i64;
        let half = 1i64 << 15;
        let magnitude = (product.abs() + half) >> 16;
        (if product < 0 { -magnitude } else { magnitude }) as i32
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowderDynamics {
    pub air_drag_keep: Scale,
    pub submerged_drag_keep: Scale,
    pub ground_friction_keep: Scale,
    pub restitution: Scale,
    pub deflect_keep: Scale,
    pub topple_start_threshold: u64,
    pub topple_keep_threshold: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiquidDynamics {
    pub air_drag_keep: Scale,
    pub submerged_drag_keep: Scale,
    pub ground_friction_keep: Scale,
    pub cohesion: Scale,
    pub restitution: Scale,
    pub deflect_keep: Scale,
    pub flow_threshold: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GasDynamics {
    pub air_drag_keep: Scale,
    pub cohesion: Scale,
    pub restitution: Scale,
    pub deflect_keep: Scale,
    pub turbulence: Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dynamics {
    None,
    Powder(PowderDynamics),
    Liquid(LiquidDynamics),
    Gas(GasDynamics),
}

#[derive(Debug, Clone, Copy)]
pub struct MaterialInfo {
    pub name: &'static str,
    pub colors: &'static [[u8; 4]],
    pub hardness: f32,
    pub mining_tier: u8,
    pub restitution: f32,
    pub surface_grip: f32,
    pub surface_bounce: f32,
    pub contact_damage: f32,
    pub emission: [f32; 3],
    pub flicker: f32,
}

pub const TIER0_MAX_HARDNESS: f32 = 0.35;
pub const TIER1_MAX_HARDNESS: f32 = 1.0;
pub const TIER2_MAX_HARDNESS: f32 = 2.0;

pub const fn mining_tier_from_hardness(hardness: f32) -> u8 {
    if hardness <= TIER0_MAX_HARDNESS {
        0
    } else if hardness <= TIER1_MAX_HARDNESS {
        1
    } else if hardness <= TIER2_MAX_HARDNESS {
        2
    } else {
        3
    }
}

pub fn per_tick_chance(rate: f32) -> f32 {
    1.0 - (-rate * (1.0 / TICK_RATE as f32)).exp()
}

pub fn per_tick_keep(rate: f32) -> f32 {
    (-rate * (1.0 / TICK_RATE as f32)).exp()
}

pub fn per_random_tick_chance(rate: f32) -> f32 {
    let opportunities = RANDOM_TICKS_PER_CHUNK as f32 / CHUNK_AREA as f32;
    (per_tick_chance(rate) / opportunities).min(1.0)
}

pub fn q16(value: f32) -> Scale {
    Scale((f64::from(value) * 65536.0).round() as u32)
}

pub fn milli(value: f32) -> i32 {
    (f64::from(value) * 1000.0).round() as i32
}
