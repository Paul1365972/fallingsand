use super::fire::{ASH, SMOKE};
use crate::{Catalog, MaterialKey, Tag, emission, flammable, inherit, material, solid};

pub const WOOD: MaterialKey = MaterialKey::new("WOOD");
pub const HEARTWOOD: MaterialKey = MaterialKey::new("HEARTWOOD");
pub const FUNGWOOD: MaterialKey = MaterialKey::new("FUNGWOOD");
pub const ROOT: MaterialKey = MaterialKey::new("ROOT");

pub const LEAVES: MaterialKey = MaterialKey::new("LEAVES");
pub const NEEDLES: MaterialKey = MaterialKey::new("NEEDLES");
pub const VINE: MaterialKey = MaterialKey::new("VINE");
pub const REED: MaterialKey = MaterialKey::new("REED");
pub const KELP: MaterialKey = MaterialKey::new("KELP");
pub const MOSS: MaterialKey = MaterialKey::new("MOSS");
pub const LICHEN: MaterialKey = MaterialKey::new("LICHEN");
pub const GRASS_BLADE: MaterialKey = MaterialKey::new("GRASS_BLADE");
pub const WILDFLOWER: MaterialKey = MaterialKey::new("WILDFLOWER");

pub const MYCELIUM: MaterialKey = MaterialKey::new("MYCELIUM");
pub const SHROOM_STEM: MaterialKey = MaterialKey::new("SHROOM_STEM");
pub const SHROOM_CAP: MaterialKey = MaterialKey::new("SHROOM_CAP");
pub const GLOWCAP: MaterialKey = MaterialKey::new("GLOWCAP");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        WOOD,
        material(solid())
            .density(700.0)
            .colors([
                [133, 94, 66, 255],
                [120, 82, 56, 255],
                [145, 104, 76, 255],
                [110, 76, 52, 255],
            ])
            .hardness(0.35)
            .restitution(0.3)
            .friction(0.5)
            .bond_group("wood")
            .tag(Tag::Dissolvable)
            .flammable(
                flammable()
                    .ignite(1.5)
                    .rate(0.25)
                    .emit(10.0)
                    .colors([
                        [255, 150, 40, 255],
                        [240, 116, 24, 255],
                        [255, 184, 64, 255],
                        [208, 92, 16, 255],
                    ])
                    .residue(ASH, 0.35)
                    .burnout(SMOKE)
                    .damage(8.0),
            ),
    );
    catalog.add(
        HEARTWOOD,
        inherit(WOOD)
            .density(820.0)
            .colors([
                [146, 82, 62, 255],
                [133, 73, 55, 255],
                [160, 93, 71, 255],
                [120, 65, 48, 255],
            ])
            .hardness(0.55)
            .flammable(
                flammable()
                    .ignite(0.8)
                    .rate(0.1)
                    .emit(10.0)
                    .residue(ASH, 0.45)
                    .burnout(SMOKE)
                    .damage(8.0),
            ),
    );
    catalog.add(
        FUNGWOOD,
        material(solid())
            .density(640.0)
            .colors([
                [176, 158, 126, 255],
                [162, 145, 115, 255],
                [190, 172, 139, 255],
                [148, 132, 104, 255],
            ])
            .hardness(0.4)
            .restitution(0.35)
            .friction(0.55)
            .bond_group("wood")
            .tag(Tag::Dissolvable),
    );
    catalog.add(
        ROOT,
        material(solid())
            .density(800.0)
            .colors([
                [104, 74, 50, 255],
                [94, 66, 44, 255],
                [116, 84, 58, 255],
                [84, 58, 38, 255],
            ])
            .hardness(0.3)
            .friction(0.6)
            .traction(0.9)
            .bond_group("wood")
            .tag(Tag::Dissolvable)
            .flammable(
                flammable()
                    .ignite(1.4)
                    .rate(0.22)
                    .emit(9.0)
                    .residue(ASH, 0.3)
                    .burnout(SMOKE)
                    .damage(7.0),
            ),
    );

    for (key, density, hardness, colors) in [
        (
            LEAVES,
            350.0,
            0.03,
            [
                [68, 138, 58, 255],
                [58, 126, 50, 255],
                [78, 150, 66, 255],
                [50, 116, 44, 255],
            ],
        ),
        (
            NEEDLES,
            330.0,
            0.03,
            [
                [46, 92, 66, 255],
                [39, 82, 58, 255],
                [55, 104, 76, 255],
                [33, 72, 50, 255],
            ],
        ),
        (
            VINE,
            400.0,
            0.05,
            [
                [78, 132, 62, 255],
                [69, 120, 55, 255],
                [90, 146, 72, 255],
                [61, 108, 48, 255],
            ],
        ),
        (
            REED,
            300.0,
            0.04,
            [
                [148, 164, 84, 255],
                [136, 151, 76, 255],
                [162, 178, 95, 255],
                [124, 138, 68, 255],
            ],
        ),
        (
            KELP,
            420.0,
            0.04,
            [
                [70, 96, 54, 255],
                [61, 86, 47, 255],
                [82, 109, 63, 255],
                [53, 76, 41, 255],
            ],
        ),
        (
            GRASS_BLADE,
            280.0,
            0.02,
            [
                [104, 168, 70, 255],
                [93, 155, 62, 255],
                [117, 182, 80, 255],
                [84, 142, 55, 255],
            ],
        ),
        (
            WILDFLOWER,
            280.0,
            0.02,
            [
                [214, 128, 178, 255],
                [232, 198, 96, 255],
                [166, 148, 226, 255],
                [236, 236, 236, 255],
            ],
        ),
    ] {
        catalog.add(
            key,
            material(solid())
                .density(density)
                .colors(colors)
                .hardness(hardness)
                .friction(0.7)
                .tag(Tag::Dissolvable)
                .flammable(
                    flammable()
                        .ignite(3.0)
                        .rate(2.2)
                        .emit(12.0)
                        .residue(ASH, 0.25)
                        .burnout(SMOKE)
                        .damage(7.0),
                ),
        );
    }

    for (key, density, colors) in [
        (
            MOSS,
            500.0,
            [
                [66, 112, 52, 255],
                [58, 102, 46, 255],
                [74, 122, 58, 255],
                [52, 94, 42, 255],
            ],
        ),
        (
            LICHEN,
            460.0,
            [
                [156, 168, 140, 255],
                [143, 155, 128, 255],
                [170, 182, 154, 255],
                [131, 142, 117, 255],
            ],
        ),
        (
            MYCELIUM,
            450.0,
            [
                [186, 176, 158, 255],
                [172, 162, 145, 255],
                [200, 190, 172, 255],
                [158, 149, 132, 255],
            ],
        ),
    ] {
        catalog.add(
            key,
            material(solid())
                .density(density)
                .colors(colors)
                .hardness(0.05)
                .restitution(0.05)
                .friction(0.8)
                .tag(Tag::Dissolvable)
                .flammable(
                    flammable()
                        .ignite(3.0)
                        .rate(2.5)
                        .emit(12.0)
                        .residue(ASH, 0.3)
                        .burnout(SMOKE)
                        .damage(7.0),
                ),
        );
    }

    catalog.add(
        SHROOM_STEM,
        material(solid())
            .density(400.0)
            .colors([
                [216, 206, 186, 255],
                [204, 194, 174, 255],
                [228, 218, 198, 255],
                [192, 182, 164, 255],
            ])
            .hardness(0.1)
            .restitution(0.6)
            .friction(0.7)
            .bond_group("shroom")
            .tag(Tag::Dissolvable)
            .flammable(
                flammable()
                    .ignite(2.0)
                    .rate(1.4)
                    .emit(10.0)
                    .residue(ASH, 0.2)
                    .burnout(SMOKE)
                    .damage(6.0),
            ),
    );
    catalog.add(
        SHROOM_CAP,
        inherit(SHROOM_STEM).density(450.0).colors([
            [176, 96, 88, 255],
            [161, 86, 79, 255],
            [192, 108, 99, 255],
            [147, 77, 70, 255],
        ]),
    );
    catalog.add(
        GLOWCAP,
        inherit(SHROOM_STEM)
            .density(450.0)
            .colors([
                [90, 220, 190, 255],
                [70, 200, 170, 255],
                [110, 240, 210, 255],
                [60, 180, 155, 255],
            ])
            .emission(emission([90, 220, 190]).intensity(0.22)),
    );
}
