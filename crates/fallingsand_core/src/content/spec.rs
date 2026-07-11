use super::MATERIAL_COUNT;
use crate::material::{Dynamics, Ember, MaterialId, Phase, Reaction};

pub trait MatSpec {
    const PHASE: Phase;
    const DENSITY_MILLI: i32;
    const IS_HOT: bool;
    const OPEN_FLAME: bool;
    const EMBER: Option<Ember>;
    const DECAY: Option<(u64, MaterialId)>;
    const IS_REACTIVE: bool;
    const DYNAMICS: Dynamics;
    const REACTIONS: &'static [Reaction; MATERIAL_COUNT];
}
