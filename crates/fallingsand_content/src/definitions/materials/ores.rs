use super::fire::{ASH, SMOKE};
use crate::{BondGroup, Catalog, MaterialKey, Tag, emission, flammable, material, solid};

pub const COAL: MaterialKey = MaterialKey::new("COAL");
pub const IRON_ORE: MaterialKey = MaterialKey::new("IRON_ORE");
pub const GOLD_ORE: MaterialKey = MaterialKey::new("GOLD_ORE");
pub const CRYSTAL: MaterialKey = MaterialKey::new("CRYSTAL");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        COAL,
        material(solid().rigid(BondGroup::Mineral))
            .density(1450.0)
            .colors([
                [52, 50, 52, 255],
                [44, 42, 44, 255],
                [62, 60, 62, 255],
                [38, 36, 40, 255],
            ])
            .hardness(0.5)
            .restitution(0.1)
            .tag(Tag::Dissolvable)
            .flammable(
                flammable()
                    .ignite(2.0)
                    .rate(0.028)
                    .emit(5.0)
                    .colors([
                        [240, 96, 28, 255],
                        [208, 68, 18, 255],
                        [255, 128, 44, 255],
                        [176, 48, 12, 255],
                    ])
                    .residue(ASH, 0.05)
                    .burnout(SMOKE)
                    .damage(8.0),
            ),
    );

    for (key, density, hardness, restitution, colors, emissive) in [
        (
            IRON_ORE,
            3200.0,
            1.4,
            0.1,
            [
                [146, 116, 96, 255],
                [132, 104, 86, 255],
                [158, 126, 104, 255],
                [120, 96, 82, 255],
            ],
            false,
        ),
        (
            GOLD_ORE,
            3600.0,
            1.6,
            0.1,
            [
                [196, 164, 62, 255],
                [180, 148, 52, 255],
                [212, 180, 76, 255],
                [166, 136, 46, 255],
            ],
            false,
        ),
        (
            CRYSTAL,
            2650.0,
            1.8,
            0.15,
            [
                [150, 220, 255, 255],
                [120, 190, 250, 255],
                [180, 240, 255, 255],
                [100, 170, 240, 255],
            ],
            true,
        ),
    ] {
        let mut definition = material(solid().rigid(BondGroup::Mineral))
            .density(density)
            .colors(colors)
            .hardness(hardness)
            .restitution(restitution)
            .tag(Tag::Dissolvable);
        if emissive {
            let [r, g, b, _] = colors[0];
            definition = definition.emission(emission([r, g, b]).intensity(0.6));
        }
        catalog.add(key, definition);
    }
}
