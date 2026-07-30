use super::fire::{ASH, SMOKE};
use crate::{Catalog, MaterialKey, Tag, flammable, material, powder, solid};

pub const TURF: MaterialKey = MaterialKey::new("TURF");
pub const DIRT: MaterialKey = MaterialKey::new("DIRT");
pub const CLAY: MaterialKey = MaterialKey::new("CLAY");
pub const PERMAFROST: MaterialKey = MaterialKey::new("PERMAFROST");
pub const BLUE_ICE: MaterialKey = MaterialKey::new("BLUE_ICE");

pub const SAND: MaterialKey = MaterialKey::new("SAND");
pub const GRAVEL: MaterialKey = MaterialKey::new("GRAVEL");
pub const MUD: MaterialKey = MaterialKey::new("MUD");
pub const SNOW: MaterialKey = MaterialKey::new("SNOW");
pub const PEAT: MaterialKey = MaterialKey::new("PEAT");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        TURF,
        material(solid())
            .density(1500.0)
            .colors([
                [86, 152, 63, 255],
                [76, 140, 55, 255],
                [96, 164, 72, 255],
                [70, 130, 50, 255],
            ])
            .hardness(0.06)
            .friction(0.75)
            .tag(Tag::Dissolvable)
            .flammable(
                flammable()
                    .ignite(4.0)
                    .rate(1.6)
                    .emit(10.0)
                    .residue(ASH, 0.25)
                    .burnout(SMOKE)
                    .damage(6.0),
            ),
    );

    catalog.add(
        DIRT,
        material(solid())
            .density(1750.0)
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
        CLAY,
        material(solid())
            .density(1900.0)
            .colors([
                [164, 116, 94, 255],
                [152, 106, 86, 255],
                [176, 126, 102, 255],
                [142, 98, 80, 255],
            ])
            .hardness(0.30)
            .friction(0.7)
            .tag(Tag::Dissolvable),
    );
    catalog.add(
        PERMAFROST,
        material(solid())
            .density(1950.0)
            .colors([
                [150, 158, 168, 255],
                [138, 147, 158, 255],
                [163, 171, 181, 255],
                [128, 136, 147, 255],
            ])
            .hardness(0.6)
            .friction(0.45)
            .tag(Tag::Dissolvable),
    );
    catalog.add(
        BLUE_ICE,
        material(solid())
            .density(917.0)
            .colors([
                [158, 200, 234, 255],
                [146, 190, 226, 255],
                [170, 210, 242, 255],
                [138, 182, 220, 255],
            ])
            .hardness(0.5)
            .restitution(0.1)
            .friction(0.03)
            .bond_group("ice")
            .traction(0.04),
    );

    for (key, phase, density, hardness, friction, colors) in [
        (
            SAND,
            powder()
                .air_drag(3.0)
                .ground_friction(48.0)
                .topple(36.0, 140.0)
                .deflect(0.45),
            1600.0,
            0.03,
            0.65,
            [
                [222, 192, 128, 255],
                [212, 182, 118, 255],
                [232, 202, 140, 255],
                [202, 172, 110, 255],
            ],
        ),
        (
            GRAVEL,
            powder()
                .air_drag(2.5)
                .ground_friction(97.0)
                .topple(13.0, 50.0)
                .deflect(0.2)
                .cohesion(0.12),
            1900.0,
            0.05,
            0.7,
            [
                [139, 133, 125, 255],
                [127, 121, 113, 255],
                [150, 144, 136, 255],
                [118, 112, 105, 255],
            ],
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
            0.02,
            0.3,
            [
                [238, 242, 248, 255],
                [230, 235, 242, 255],
                [245, 248, 252, 255],
                [222, 228, 238, 255],
            ],
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
        .traction(0.45)
        .tag(Tag::Dissolvable),
    );

    catalog.add(
        PEAT,
        material(
            powder()
                .air_drag(4.5)
                .ground_friction(100.0)
                .topple(14.0, 56.0)
                .deflect(0.2)
                .cohesion(0.45),
        )
        .density(700.0)
        .colors([
            [72, 54, 40, 255],
            [64, 47, 35, 255],
            [82, 63, 47, 255],
            [56, 41, 30, 255],
        ])
        .hardness(0.06)
        .friction(0.8)
        .tag(Tag::Dissolvable)
        .flammable(
            flammable()
                .ignite(1.2)
                .sealed_burn(0.6)
                .rate(0.035)
                .emit(3.0)
                .colors([
                    [188, 84, 28, 255],
                    [160, 66, 20, 255],
                    [212, 104, 38, 255],
                    [136, 52, 14, 255],
                ])
                .residue(ASH, 0.15)
                .burnout(SMOKE)
                .damage(6.0),
        ),
    );
}
