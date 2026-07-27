use super::fire::{ASH, SMOKE};
use crate::{Catalog, MaterialKey, Tag, flammable, material, powder, solid};

pub const STONE: MaterialKey = MaterialKey::new("STONE");
pub const DIRT: MaterialKey = MaterialKey::new("DIRT");
pub const GRASS: MaterialKey = MaterialKey::new("GRASS");
pub const GRAVEL: MaterialKey = MaterialKey::new("GRAVEL");
pub const SAND: MaterialKey = MaterialKey::new("SAND");
pub const SNOW: MaterialKey = MaterialKey::new("SNOW");
pub const ICE: MaterialKey = MaterialKey::new("ICE");
pub const MUD: MaterialKey = MaterialKey::new("MUD");
pub const CLAY: MaterialKey = MaterialKey::new("CLAY");
pub const SANDSTONE: MaterialKey = MaterialKey::new("SANDSTONE");
pub const DEEPSTONE: MaterialKey = MaterialKey::new("DEEPSTONE");
pub const BASALT: MaterialKey = MaterialKey::new("BASALT");
pub const BRICK: MaterialKey = MaterialKey::new("BRICK");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        STONE,
        material(solid())
            .density(2600.0)
            .colors([
                [110, 110, 115, 255],
                [100, 100, 105, 255],
                [120, 120, 125, 255],
                [95, 95, 100, 255],
            ])
            .hardness(0.9)
            .restitution(0.15)
            .friction(0.55)
            .tag(Tag::Dissolvable),
    );
    catalog.add(
        DIRT,
        material(solid())
            .density(1800.0)
            .colors([
                [121, 85, 58, 255],
                [112, 78, 52, 255],
                [130, 92, 64, 255],
                [105, 72, 48, 255],
            ])
            .hardness(0.08)
            .friction(0.75)
            .tag(Tag::Dissolvable),
    );
    catalog.add(
        GRASS,
        material(solid())
            .density(1600.0)
            .colors([
                [86, 152, 63, 255],
                [76, 140, 55, 255],
                [96, 164, 72, 255],
                [70, 130, 50, 255],
            ])
            .hardness(0.08)
            .tag(Tag::Dissolvable)
            .flammable(
                flammable()
                    .ignite(4.0)
                    .rate(1.2)
                    .emit(10.0)
                    .residue(ASH, 0.3)
                    .burnout(SMOKE)
                    .damage(7.0),
            ),
    );

    for (key, phase, density, colors, hardness, friction) in [
        (
            GRAVEL,
            powder()
                .air_drag(2.5)
                .ground_friction(97.0)
                .topple(13.0, 50.0)
                .deflect(0.2)
                .cohesion(0.12),
            1900.0,
            [
                [139, 133, 125, 255],
                [127, 121, 113, 255],
                [150, 144, 136, 255],
                [118, 112, 105, 255],
            ],
            0.05,
            0.7,
        ),
        (
            SAND,
            powder()
                .air_drag(3.0)
                .ground_friction(48.0)
                .topple(36.0, 140.0)
                .deflect(0.45),
            1600.0,
            [
                [222, 192, 128, 255],
                [212, 182, 118, 255],
                [232, 202, 140, 255],
                [202, 172, 110, 255],
            ],
            0.03,
            0.65,
        ),
        (
            SNOW,
            powder()
                .air_drag(8.0)
                .ground_friction(36.0)
                .topple(48.0, 190.0)
                .deflect(0.55)
                .cohesion(0.3),
            300.0,
            [
                [238, 242, 248, 255],
                [230, 235, 242, 255],
                [245, 248, 252, 255],
                [222, 228, 238, 255],
            ],
            0.02,
            0.3,
        ),
    ] {
        catalog.add(
            key,
            material(phase)
                .density(density)
                .colors(colors)
                .hardness(hardness)
                .friction(friction)
                .tag(Tag::Dissolvable),
        );
    }

    catalog.add(
        ICE,
        material(solid())
            .density(917.0)
            .colors([
                [158, 200, 234, 255],
                [146, 190, 226, 255],
                [170, 210, 242, 255],
                [138, 182, 220, 255],
            ])
            .hardness(0.4)
            .restitution(0.1)
            .friction(0.05)
            .bond_group("ice")
            .entity_grip(0.05)
            .tag(Tag::Dissolvable),
    );
    catalog.add(
        MUD,
        material(
            powder()
                .air_drag(4.0)
                .ground_friction(114.0)
                .topple(10.0, 40.0)
                .deflect(0.15)
                .cohesion(0.65),
        )
        .density(1700.0)
        .colors([
            [92, 72, 52, 255],
            [84, 64, 46, 255],
            [100, 80, 58, 255],
            [76, 58, 42, 255],
        ])
        .hardness(0.05)
        .friction(0.85)
        .tag(Tag::Dissolvable),
    );
    catalog.add(
        CLAY,
        material(solid())
            .density(1900.0)
            .colors([
                [164, 116, 94, 255],
                [152, 106, 86, 255],
                [176, 126, 102, 255],
                [142, 98, 80, 255],
            ])
            .hardness(0.3)
            .friction(0.7)
            .tag(Tag::Dissolvable),
    );
    catalog.add(
        SANDSTONE,
        material(solid())
            .density(2300.0)
            .colors([
                [208, 178, 128, 255],
                [196, 166, 118, 255],
                [218, 190, 140, 255],
                [186, 156, 110, 255],
            ])
            .hardness(0.6)
            .restitution(0.12)
            .friction(0.55)
            .tag(Tag::Dissolvable),
    );
    catalog.add(
        DEEPSTONE,
        material(solid())
            .density(2900.0)
            .colors([
                [82, 82, 92, 255],
                [74, 74, 84, 255],
                [90, 90, 100, 255],
                [66, 66, 76, 255],
            ])
            .hardness(2.5)
            .restitution(0.15)
            .friction(0.6),
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
            .hardness(3.5)
            .restitution(0.15)
            .friction(0.6),
    );
    catalog.add(
        BRICK,
        material(solid())
            .density(2400.0)
            .colors([
                [156, 90, 74, 255],
                [144, 80, 66, 255],
                [168, 100, 82, 255],
                [132, 72, 60, 255],
            ])
            .hardness(1.1)
            .restitution(0.15)
            .friction(0.7)
            .bond_group("brick")
            .tag(Tag::Dissolvable),
    );
}
