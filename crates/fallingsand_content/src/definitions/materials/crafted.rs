use super::fire::{ASH, SMOKE};
use crate::{Catalog, MaterialKey, Tag, emission, flammable, material, powder, solid};

pub const PLANKS: MaterialKey = MaterialKey::new("PLANKS");
pub const BEAM: MaterialKey = MaterialKey::new("BEAM");
pub const BRICK: MaterialKey = MaterialKey::new("BRICK");
pub const GLASS: MaterialKey = MaterialKey::new("GLASS");
pub const IRON: MaterialKey = MaterialKey::new("IRON");
pub const RUST: MaterialKey = MaterialKey::new("RUST");
pub const BRONZE: MaterialKey = MaterialKey::new("BRONZE");
pub const STEEL: MaterialKey = MaterialKey::new("STEEL");
pub const ROPE: MaterialKey = MaterialKey::new("ROPE");
pub const ROCKWOOL: MaterialKey = MaterialKey::new("ROCKWOOL");
pub const GUNPOWDER: MaterialKey = MaterialKey::new("GUNPOWDER");
pub const TORCH: MaterialKey = MaterialKey::new("TORCH");
pub const LUMEN_LAMP: MaterialKey = MaterialKey::new("LUMEN_LAMP");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        PLANKS,
        material(solid())
            .density(600.0)
            .colors([
                [172, 132, 86, 255],
                [162, 122, 78, 255],
                [182, 142, 94, 255],
                [152, 114, 72, 255],
            ])
            .hardness(0.3)
            .restitution(0.3)
            .friction(0.45)
            .bond_group("timber")
            .tag(Tag::Dissolvable)
            .flammable(
                flammable()
                    .ignite(1.5)
                    .rate(0.25)
                    .emit(10.0)
                    .residue(ASH, 0.35)
                    .burnout(SMOKE)
                    .damage(8.0),
            ),
    );
    catalog.add(
        BEAM,
        material(solid())
            .density(680.0)
            .colors([
                [128, 96, 62, 255],
                [118, 88, 56, 255],
                [140, 106, 70, 255],
                [108, 80, 50, 255],
            ])
            .hardness(0.45)
            .restitution(0.25)
            .friction(0.5)
            .bond_group("timber")
            .tag(Tag::Dissolvable)
            .flammable(
                flammable()
                    .ignite(1.2)
                    .rate(0.16)
                    .emit(10.0)
                    .residue(ASH, 0.4)
                    .burnout(SMOKE)
                    .damage(8.0),
            ),
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
    catalog.add(
        GLASS,
        material(solid())
            .density(2500.0)
            .colors([
                [196, 220, 232, 160],
                [182, 208, 222, 160],
                [210, 232, 242, 160],
                [168, 196, 212, 160],
            ])
            .hardness(0.9)
            .restitution(0.2)
            .friction(0.3)
            .bond_group("glass"),
    );

    for (key, density, hardness, colors) in [
        (
            IRON,
            7800.0,
            2.0,
            [
                [166, 168, 174, 255],
                [152, 154, 160, 255],
                [182, 184, 190, 255],
                [138, 140, 146, 255],
            ],
        ),
        (
            BRONZE,
            8800.0,
            1.9,
            [
                [186, 138, 78, 255],
                [170, 124, 68, 255],
                [204, 154, 92, 255],
                [154, 110, 58, 255],
            ],
        ),
        (
            STEEL,
            7900.0,
            2.6,
            [
                [128, 134, 146, 255],
                [116, 122, 134, 255],
                [144, 150, 162, 255],
                [104, 110, 122, 255],
            ],
        ),
    ] {
        catalog.add(
            key,
            material(solid())
                .density(density)
                .colors(colors)
                .hardness(hardness)
                .restitution(0.2)
                .friction(0.5)
                .bond_group("metal"),
        );
    }

    catalog.add(
        RUST,
        material(
            powder()
                .air_drag(3.6)
                .ground_friction(70.0)
                .topple(26.0, 100.0)
                .deflect(0.3)
                .cohesion(0.08),
        )
        .density(5200.0)
        .colors([
            [150, 82, 46, 255],
            [136, 73, 40, 255],
            [166, 94, 55, 255],
            [122, 64, 34, 255],
        ])
        .hardness(0.2)
        .friction(0.7)
        .tag(Tag::Dissolvable),
    );

    catalog.add(
        ROPE,
        material(solid())
            .density(200.0)
            .colors([
                [186, 160, 110, 255],
                [172, 147, 100, 255],
                [200, 175, 124, 255],
                [158, 134, 90, 255],
            ])
            .hardness(0.02)
            .restitution(0.05)
            .friction(0.1)
            .traction(1.0)
            .bond_group("rope")
            .tag(Tag::Dissolvable)
            .flammable(
                flammable()
                    .ignite(6.0)
                    .rate(2.2)
                    .emit(8.0)
                    .burnout(SMOKE)
                    .damage(5.0),
            ),
    );
    catalog.add(
        ROCKWOOL,
        material(solid())
            .density(200.0)
            .colors([
                [188, 180, 168, 255],
                [174, 166, 155, 255],
                [202, 194, 182, 255],
                [160, 152, 142, 255],
            ])
            .hardness(0.1)
            .friction(0.8),
    );

    catalog.add(
        GUNPOWDER,
        material(
            powder()
                .air_drag(3.8)
                .ground_friction(52.0)
                .topple(34.0, 130.0)
                .deflect(0.42),
        )
        .density(1700.0)
        .colors([
            [58, 56, 60, 255],
            [50, 48, 52, 255],
            [68, 66, 70, 255],
            [43, 41, 45, 255],
        ])
        .hardness(0.05)
        .friction(0.55)
        .flammable(
            flammable()
                .ignite(80.0)
                .sealed_burn(1.0)
                .rate(60.0)
                .emit(40.0)
                .colors([
                    [255, 236, 180, 255],
                    [255, 200, 110, 255],
                    [255, 250, 224, 255],
                    [248, 168, 64, 255],
                ])
                .burnout(SMOKE)
                .damage(20.0),
        ),
    );

    catalog.add(
        TORCH,
        material(solid())
            .density(400.0)
            .colors([
                [180, 132, 76, 255],
                [166, 120, 68, 255],
                [196, 146, 86, 255],
            ])
            .hardness(0.02)
            .friction(0.5)
            .tag(Tag::Dissolvable)
            .emission(emission([255, 168, 72]).intensity(1.6).flicker(0.35))
            .flammable(
                flammable()
                    .ignite(4.0)
                    .rate(0.004)
                    .emit(2.0)
                    .burnout(ASH)
                    .damage(4.0),
            ),
    );
    catalog.add(
        LUMEN_LAMP,
        material(solid())
            .density(1200.0)
            .colors([
                [168, 236, 248, 255],
                [148, 222, 238, 255],
                [196, 246, 254, 255],
            ])
            .hardness(0.3)
            .friction(0.4)
            .bond_group("glass")
            .emission(emission([150, 230, 246]).intensity(1.5)),
    );
}
