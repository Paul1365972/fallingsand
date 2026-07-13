use super::materials::fire::SMOKE;
use super::materials::fluids::{ACID, LAVA, STEAM, WATER};
use super::materials::special::AIR;
use super::materials::terrain::{ICE, SNOW, STONE};
use crate::{Catalog, Tag, reaction, same, tagged};

pub fn define(catalog: &mut Catalog) {
    catalog.react(reaction(LAVA, WATER).becomes(STONE, STEAM).rate(97.0));
    catalog.react(
        reaction(ACID, tagged(Tag::Dissolvable))
            .becomes(AIR, AIR)
            .rate(0.8),
    );
    catalog.react(
        reaction(SNOW, tagged(Tag::Hot))
            .becomes(WATER, same(Tag::Hot))
            .rate(3.0),
    );
    catalog.react(reaction(SNOW, LAVA).becomes(STEAM, LAVA).rate(20.0));
    catalog.react(
        reaction(ICE, tagged(Tag::Hot))
            .becomes(WATER, same(Tag::Hot))
            .rate(1.5),
    );
    catalog.react(reaction(ICE, LAVA).becomes(WATER, LAVA).rate(10.0));

    catalog.decay(STEAM, WATER, 0.1);
    catalog.decay(SMOKE, AIR, 0.36);
}
