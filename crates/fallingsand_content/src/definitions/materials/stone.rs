use super::fire::{ASH, SMOKE};
use crate::{Catalog, MaterialKey, Tag, emission, flammable, material, powder, solid};

pub const LIMESTONE: MaterialKey = MaterialKey::new("LIMESTONE");
pub const SANDSTONE: MaterialKey = MaterialKey::new("SANDSTONE");
pub const SHALE: MaterialKey = MaterialKey::new("SHALE");
pub const GRANITE: MaterialKey = MaterialKey::new("GRANITE");
pub const MYCOSTONE: MaterialKey = MaterialKey::new("MYCOSTONE");
pub const BASALT: MaterialKey = MaterialKey::new("BASALT");
pub const OBSIDIAN: MaterialKey = MaterialKey::new("OBSIDIAN");
pub const BONE: MaterialKey = MaterialKey::new("BONE");
pub const TITANFLESH: MaterialKey = MaterialKey::new("TITANFLESH");
pub const RUBBLE: MaterialKey = MaterialKey::new("RUBBLE");
pub const CONCRETE: MaterialKey = MaterialKey::new("CONCRETE");

pub fn define(catalog: &mut Catalog) {
    for (key, density, hardness, friction, colors) in [
        (
            LIMESTONE,
            2500.0,
            0.7,
            0.55,
            [
                [186, 180, 162, 255],
                [173, 167, 150, 255],
                [199, 193, 175, 255],
                [161, 155, 139, 255],
            ],
        ),
        (
            SANDSTONE,
            2300.0,
            0.6,
            0.55,
            [
                [208, 178, 128, 255],
                [196, 166, 118, 255],
                [218, 190, 140, 255],
                [186, 156, 110, 255],
            ],
        ),
        (
            CONCRETE,
            2350.0,
            0.8,
            0.62,
            [
                [163, 159, 151, 255],
                [150, 146, 139, 255],
                [178, 174, 165, 255],
                [138, 134, 128, 255],
            ],
        ),
        (
            GRANITE,
            2700.0,
            1.6,
            0.6,
            [
                [140, 132, 130, 255],
                [128, 120, 119, 255],
                [153, 145, 142, 255],
                [116, 109, 108, 255],
            ],
        ),
    ] {
        catalog.add(
            key,
            material(solid())
                .density(density)
                .colors(colors)
                .hardness(hardness)
                .restitution(0.14)
                .friction(friction)
                .tag(Tag::Dissolvable),
        );
    }

    catalog.add(
        SHALE,
        material(solid())
            .density(2400.0)
            .colors([
                [92, 96, 102, 255],
                [83, 87, 93, 255],
                [103, 107, 113, 255],
                [75, 78, 84, 255],
            ])
            .hardness(0.35)
            .restitution(0.12)
            .friction(0.55)
            .bond_group("shale")
            .tag(Tag::Dissolvable),
    );

    catalog.add(
        MYCOSTONE,
        material(solid())
            .density(2300.0)
            .colors([
                [86, 96, 92, 255],
                [78, 88, 84, 255],
                [96, 107, 102, 255],
                [70, 79, 76, 255],
            ])
            .hardness(0.9)
            .restitution(0.14)
            .friction(0.6)
            .tag(Tag::Dissolvable)
            .emission(emission([96, 188, 150]).intensity(0.06)),
    );

    catalog.add(
        BASALT,
        material(solid())
            .density(3000.0)
            .colors([
                [56, 52, 58, 255],
                [48, 45, 52, 255],
                [64, 60, 66, 255],
                [42, 39, 46, 255],
            ])
            .hardness(2.4)
            .restitution(0.15)
            .friction(0.6),
    );
    catalog.add(
        OBSIDIAN,
        material(solid())
            .density(2600.0)
            .colors([
                [30, 26, 38, 255],
                [24, 21, 31, 255],
                [38, 33, 47, 255],
                [19, 16, 25, 255],
            ])
            .hardness(1.8)
            .restitution(0.2)
            .friction(0.35)
            .contact_damage(4.0),
    );

    catalog.add(
        BONE,
        material(solid())
            .density(1800.0)
            .colors([
                [214, 206, 186, 255],
                [200, 192, 172, 255],
                [228, 220, 201, 255],
                [186, 178, 159, 255],
            ])
            .hardness(1.2)
            .restitution(0.2)
            .friction(0.55)
            .bond_group("bone")
            .flammable(
                flammable()
                    .ignite(1.0)
                    .rate(0.06)
                    .emit(4.0)
                    .residue(ASH, 0.4)
                    .burnout(SMOKE)
                    .damage(6.0),
            ),
    );
    catalog.add(
        TITANFLESH,
        material(solid())
            .density(1100.0)
            .colors([
                [126, 66, 78, 255],
                [113, 58, 69, 255],
                [140, 76, 89, 255],
                [100, 50, 60, 255],
            ])
            .hardness(0.4)
            .friction(0.8)
            .traction(0.7)
            .flammable(
                flammable()
                    .ignite(2.0)
                    .rate(0.3)
                    .emit(8.0)
                    .residue(ASH, 0.2)
                    .burnout(SMOKE)
                    .damage(7.0),
            ),
    );

    catalog.add(
        RUBBLE,
        material(
            powder()
                .air_drag(2.4)
                .ground_friction(100.0)
                .topple(12.0, 46.0)
                .deflect(0.18)
                .cohesion(0.1),
        )
        .density(1950.0)
        .colors([
            [124, 120, 116, 255],
            [113, 109, 105, 255],
            [136, 132, 128, 255],
            [102, 98, 95, 255],
        ])
        .hardness(0.05)
        .friction(0.7)
        .tag(Tag::Dissolvable),
    );
}
