use super::fire::{ASH, SMOKE};
use crate::{Catalog, MaterialKey, Tag, emission, flammable, material, powder, solid};

pub const COAL: MaterialKey = MaterialKey::new("COAL");
pub const NATIVE_COPPER: MaterialKey = MaterialKey::new("NATIVE_COPPER");
pub const VERDIGRIS: MaterialKey = MaterialKey::new("VERDIGRIS");
pub const TIN_ORE: MaterialKey = MaterialKey::new("TIN_ORE");
pub const IRON_ORE: MaterialKey = MaterialKey::new("IRON_ORE");
pub const GOLD: MaterialKey = MaterialKey::new("GOLD");
pub const QUARTZ: MaterialKey = MaterialKey::new("QUARTZ");
pub const FLINT: MaterialKey = MaterialKey::new("FLINT");
pub const AMBER: MaterialKey = MaterialKey::new("AMBER");
pub const GYPSUM: MaterialKey = MaterialKey::new("GYPSUM");
pub const LUMEN: MaterialKey = MaterialKey::new("LUMEN");

pub const SULFUR: MaterialKey = MaterialKey::new("SULFUR");
pub const SALTPETER: MaterialKey = MaterialKey::new("SALTPETER");
pub const SALT: MaterialKey = MaterialKey::new("SALT");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        COAL,
        material(solid())
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
                    .sealed_burn(0.4)
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

    for (key, density, hardness, colors) in [
        (
            NATIVE_COPPER,
            3100.0,
            0.9,
            [
                [196, 114, 74, 255],
                [180, 102, 65, 255],
                [212, 130, 88, 255],
                [166, 92, 57, 255],
            ],
        ),
        (
            VERDIGRIS,
            2800.0,
            0.3,
            [
                [96, 176, 148, 255],
                [84, 162, 135, 255],
                [112, 192, 163, 255],
                [72, 146, 121, 255],
            ],
        ),
        (
            TIN_ORE,
            3000.0,
            1.0,
            [
                [148, 152, 158, 255],
                [135, 139, 145, 255],
                [163, 167, 173, 255],
                [122, 126, 132, 255],
            ],
        ),
        (
            IRON_ORE,
            3200.0,
            1.4,
            [
                [146, 116, 96, 255],
                [132, 104, 86, 255],
                [158, 126, 104, 255],
                [120, 96, 82, 255],
            ],
        ),
        (
            GOLD,
            3600.0,
            1.0,
            [
                [212, 176, 66, 255],
                [196, 160, 56, 255],
                [228, 194, 84, 255],
                [180, 146, 48, 255],
            ],
        ),
        (
            QUARTZ,
            2650.0,
            1.7,
            [
                [230, 228, 232, 255],
                [216, 214, 220, 255],
                [242, 241, 245, 255],
                [202, 200, 207, 255],
            ],
        ),
        (
            FLINT,
            2600.0,
            0.8,
            [
                [64, 68, 74, 255],
                [56, 60, 66, 255],
                [76, 80, 86, 255],
                [48, 51, 57, 255],
            ],
        ),
        (
            GYPSUM,
            2300.0,
            0.4,
            [
                [232, 228, 220, 255],
                [219, 215, 207, 255],
                [243, 240, 234, 255],
                [206, 202, 194, 255],
            ],
        ),
    ] {
        catalog.add(
            key,
            material(solid())
                .density(density)
                .colors(colors)
                .hardness(hardness)
                .restitution(0.12)
                .friction(0.55)
                .tag(Tag::Dissolvable),
        );
    }

    catalog.add(
        AMBER,
        material(solid())
            .density(1050.0)
            .colors([
                [222, 148, 52, 255],
                [204, 132, 43, 255],
                [238, 168, 70, 255],
                [186, 116, 35, 255],
            ])
            .hardness(0.7)
            .restitution(0.25)
            .tag(Tag::Dissolvable)
            .emission(emission([222, 148, 52]).intensity(0.2))
            .flammable(
                flammable()
                    .ignite(3.0)
                    .rate(0.12)
                    .emit(10.0)
                    .burnout(SMOKE)
                    .damage(7.0),
            ),
    );

    catalog.add(
        LUMEN,
        material(solid())
            .density(2600.0)
            .colors([
                [150, 232, 246, 255],
                [124, 214, 234, 255],
                [186, 244, 252, 255],
                [102, 194, 218, 255],
            ])
            .hardness(1.9)
            .restitution(0.16)
            .tag(Tag::Dissolvable)
            .emission(emission([140, 226, 244]).intensity(0.30)),
    );

    for (key, density, hardness, colors, soluble) in [
        (
            SULFUR,
            2000.0,
            0.2,
            [
                [226, 206, 72, 255],
                [210, 190, 62, 255],
                [240, 222, 92, 255],
                [194, 174, 52, 255],
            ],
            false,
        ),
        (
            SALTPETER,
            2100.0,
            0.15,
            [
                [226, 222, 210, 255],
                [212, 208, 196, 255],
                [240, 236, 226, 255],
                [198, 194, 182, 255],
            ],
            false,
        ),
        (
            SALT,
            2160.0,
            0.3,
            [
                [238, 236, 232, 255],
                [226, 224, 220, 255],
                [248, 247, 244, 255],
                [214, 212, 208, 255],
            ],
            true,
        ),
    ] {
        let mut definition = material(
            powder()
                .air_drag(3.4)
                .ground_friction(64.0)
                .topple(28.0, 108.0)
                .deflect(0.35)
                .cohesion(0.06),
        )
        .density(density)
        .colors(colors)
        .hardness(hardness)
        .friction(0.6)
        .tag(Tag::Dissolvable);
        if soluble {
            definition = definition.tag(Tag::Soluble);
        }
        catalog.add(key, definition);
    }
}
