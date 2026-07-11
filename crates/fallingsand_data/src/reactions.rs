use crate::material::*;
use fallingsand_core::Tag::*;

reactions! {
    LAVA + WATER => STONE + STEAM @ 97.0;
    ACID + [Dissolvable] => AIR + AIR @ 0.8;
    SNOW + FIRE => WATER + FIRE @ 3.0;
    SNOW + LAVA => STEAM + LAVA @ 20.0;
    ICE + FIRE => WATER + FIRE @ 1.5;
    ICE + LAVA => WATER + LAVA @ 10.0;
}
