use super::MATERIAL_COUNT;
use crate::material::{Burning, Dynamics, Ignition, MaterialId, Phase, Reaction};

pub trait MatSpec {
    const PHASE: Phase;
    const DENSITY_MILLI: i32;
    const IS_HOT: bool;
    const IGNITION: Option<Ignition>;
    const BURNING: Option<Burning>;
    const DECAY: Option<(u64, MaterialId)>;
    const IS_REACTIVE: bool;
    const DYNAMICS: Dynamics;
    const REACTIONS: &'static [Reaction; MATERIAL_COUNT];
}
