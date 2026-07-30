use super::materials::crafted::{GLASS, IRON, RUST, STEEL};
use super::materials::fire::{CHARCOAL, FIRE, SMOKE};
use super::materials::flora::{GLOWCAP, MYCELIUM};
use super::materials::fluids::{
    ACID, ALCOHOL, BLOOD, BRINE, CEMENT, ICHOR, LAVA, MOLTEN_GLASS, MOLTEN_IRON, OIL, RESIN, SLIME,
    TOXIC_SLUDGE, WATER,
};
use super::materials::gases::{CHOKEDAMP, FIREDAMP, SPOREGAS, STEAM, TOXIC_GAS, VOIDMIST};
use super::materials::ores::{AMBER, IRON_ORE, SALT, SULFUR};
use super::materials::soil::{BLUE_ICE, CLAY, MUD, PERMAFROST, SAND, SNOW};
use super::materials::special::{AIR, CORPSE};
use super::materials::stone::{CONCRETE, LIMESTONE, OBSIDIAN};
use crate::{Catalog, Tag, reaction, same, tagged};

pub fn define(catalog: &mut Catalog) {
    catalog.react(reaction(LAVA, WATER).becomes(OBSIDIAN, STEAM).rate(97.0));
    catalog.react(reaction(LAVA, BRINE).becomes(OBSIDIAN, STEAM).rate(97.0));

    catalog.react(
        reaction(LAVA, IRON_ORE)
            .becomes(LAVA, MOLTEN_IRON)
            .rate(1.2),
    );
    catalog.react(reaction(MOLTEN_IRON, WATER).becomes(IRON, STEAM).rate(40.0));
    catalog.react(
        reaction(MOLTEN_IRON, CHARCOAL)
            .becomes(STEEL, SMOKE)
            .rate(0.5),
    );
    catalog.react(reaction(LAVA, SAND).becomes(LAVA, MOLTEN_GLASS).rate(1.0));
    catalog.react(
        reaction(MOLTEN_GLASS, WATER)
            .becomes(GLASS, STEAM)
            .rate(40.0),
    );

    catalog.react(reaction(ACID, LIMESTONE).becomes(AIR, CHOKEDAMP).rate(6.0));
    catalog.react(
        reaction(ACID, tagged(Tag::Dissolvable))
            .becomes(AIR, AIR)
            .rate(0.8),
    );
    catalog.react(reaction(ACID, IRON).becomes(AIR, FIREDAMP).rate(2.0));
    catalog.react(reaction(ACID, OIL).becomes(ACID, AIR).rate(0.5));

    catalog.react(
        reaction(TOXIC_SLUDGE, tagged(Tag::Hot))
            .becomes(TOXIC_GAS, same(Tag::Hot))
            .rate(9.0),
    );
    catalog.react(
        reaction(TOXIC_SLUDGE, FIRE)
            .becomes(TOXIC_GAS, SMOKE)
            .rate(14.0),
    );
    catalog.react(reaction(TOXIC_GAS, WATER).becomes(AIR, WATER).rate(0.6));

    catalog.react(reaction(CEMENT, WATER).becomes(CONCRETE, AIR).rate(6.0));
    catalog.react(reaction(CEMENT, AIR).becomes(CONCRETE, AIR).rate(0.05));
    catalog.react(
        reaction(CEMENT, tagged(Tag::Hot))
            .becomes(CONCRETE, same(Tag::Hot))
            .rate(20.0),
    );

    catalog.react(reaction(SLIME, FIRE).becomes(SMOKE, FIRE).rate(4.0));
    catalog.react(reaction(SLIME, ACID).becomes(AIR, ACID).rate(3.0));

    catalog.react(
        reaction(BLOOD, tagged(Tag::Hot))
            .becomes(SMOKE, same(Tag::Hot))
            .rate(12.0),
    );

    catalog.react(reaction(ALCOHOL, WATER).becomes(WATER, WATER).rate(0.08));

    catalog.react(
        reaction(CHOKEDAMP, FIRE)
            .becomes(CHOKEDAMP, SMOKE)
            .rate(20.0),
    );
    catalog.react(reaction(VOIDMIST, FIRE).becomes(VOIDMIST, AIR).rate(60.0));
    catalog.react(reaction(CHOKEDAMP, WATER).becomes(AIR, WATER).rate(0.5));
    catalog.react(reaction(SPOREGAS, WATER).becomes(AIR, WATER).rate(0.5));

    catalog.react(
        reaction(SULFUR, tagged(Tag::Hot))
            .becomes(TOXIC_GAS, same(Tag::Hot))
            .rate(3.0),
    );
    catalog.react(reaction(TOXIC_GAS, STEAM).becomes(ACID, ACID).rate(1.5));

    catalog.react(reaction(SALT, WATER).becomes(AIR, BRINE).rate(4.0));
    catalog.react(
        reaction(BRINE, tagged(Tag::Hot))
            .becomes(SALT, same(Tag::Hot))
            .rate(8.0),
    );
    catalog.react(reaction(BRINE, IRON).becomes(BRINE, RUST).rate(0.15));
    catalog.react(reaction(BRINE, STEEL).becomes(BRINE, RUST).rate(0.05));

    catalog.react(
        reaction(MUD, tagged(Tag::Hot))
            .becomes(CLAY, same(Tag::Hot))
            .rate(2.0),
    );
    catalog.react(
        reaction(SNOW, tagged(Tag::Hot))
            .becomes(WATER, same(Tag::Hot))
            .rate(3.0),
    );
    catalog.react(
        reaction(BLUE_ICE, tagged(Tag::Hot))
            .becomes(WATER, same(Tag::Hot))
            .rate(1.5),
    );
    catalog.react(
        reaction(PERMAFROST, tagged(Tag::Hot))
            .becomes(MUD, same(Tag::Hot))
            .rate(0.4),
    );

    catalog.react(reaction(RESIN, AIR).becomes(AMBER, AIR).rate(0.02));
    catalog.react(reaction(MYCELIUM, WATER).becomes(GLOWCAP, WATER).rate(0.02));
    catalog.react(reaction(MYCELIUM, SLIME).becomes(GLOWCAP, SLIME).rate(0.05));

    catalog.decay(STEAM, WATER, 0.1);
    catalog.decay(SMOKE, AIR, 0.36);
    catalog.decay(CORPSE, SMOKE, 0.15);
    catalog.decay(FIREDAMP, AIR, 0.004);
    catalog.decay(CHOKEDAMP, AIR, 0.002);
    catalog.decay(SPOREGAS, AIR, 0.01);
    catalog.decay(TOXIC_GAS, AIR, 0.006);
    catalog.decay(ICHOR, SMOKE, 0.01);
    catalog.decay(BLOOD, AIR, 0.004);
}
