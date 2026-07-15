use serde::{Deserialize, Serialize};

pub const TICK_RATE: u32 = 60;
pub const VEL_ONE: i32 = 1024;
pub const CHUNK_SIZE: usize = 64;
pub const CHUNK_AREA: usize = CHUNK_SIZE * CHUNK_SIZE;

pub const RANDOM_TICKS_PER_CHUNK: u32 = 16;

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

impl Tag {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "Dissolvable" => Self::Dissolvable,
            "Hot" => Self::Hot,
            "Player" => Self::Player,
            _ => return None,
        })
    }
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
    pub open_random: u64,
    pub sealed_random: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmberKind {
    Flame,
    Fuel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ember {
    pub burn: u64,
    pub burn_sealed: u64,
    pub emit: u64,
    pub residue: Option<(u64, MaterialId)>,
    pub burnout: MaterialId,
    pub kind: EmberKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowderDynamics {
    pub drag_keep_q16: u32,
    pub drag_keep_submerged_q16: u32,
    pub friction_keep_q16: u32,
    pub cohesion_q16: u32,
    pub restitution_q16: u32,
    pub redirect_keep_q16: u32,
    pub slide_threshold: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiquidDynamics {
    pub drag_keep_q16: u32,
    pub drag_keep_submerged_q16: u32,
    pub friction_keep_q16: u32,
    pub cohesion_q16: u32,
    pub restitution_q16: u32,
    pub redirect_keep_q16: u32,
    pub flow_threshold: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GasDynamics {
    pub drag_keep_q16: u32,
    pub cohesion_q16: u32,
    pub restitution_q16: u32,
    pub redirect_keep_q16: u32,
    pub turbulence_q16: u32,
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

/// `per_tick_chance` divided by the per-cell sampling rate, preserving a seconds-rate under random
/// ticks. Clamped at 1.0.
pub fn per_random_tick_chance(rate: f32) -> f32 {
    let opportunities = RANDOM_TICKS_PER_CHUNK as f32 / CHUNK_AREA as f32;
    (per_tick_chance(rate) / opportunities).min(1.0)
}

pub fn q16(value: f32) -> u32 {
    (f64::from(value) * 65536.0).round() as u32
}

pub fn milli(value: f32) -> i32 {
    (f64::from(value) * 1000.0).round() as i32
}
