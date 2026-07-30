use crate::{Catalog, MaterialKey, Tag, burning, emission, flammable, gas, material, powder};

pub const FIRE: MaterialKey = MaterialKey::new("FIRE");
pub const SMOKE: MaterialKey = MaterialKey::new("SMOKE");
pub const ASH: MaterialKey = MaterialKey::new("ASH");
pub const CHARCOAL: MaterialKey = MaterialKey::new("CHARCOAL");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        FIRE,
        material(gas().air_drag(5.5).turbulence(65.0))
            .density(0.3)
            .colors([
                [255, 160, 32, 255],
                [255, 120, 16, 255],
                [255, 200, 64, 255],
                [232, 88, 8, 255],
            ])
            .burning(burning().rate(2.8).burnout(SMOKE))
            .contact_damage(8.0)
            .tags([Tag::Hot])
            .emission(emission([255, 140, 32]).intensity(3.5).flicker(0.5)),
    );
    catalog.add(
        SMOKE,
        material(gas().air_drag(7.0).turbulence(90.0))
            .density(0.4)
            .colors([[60, 58, 56, 140], [52, 50, 48, 120], [70, 68, 66, 150]]),
    );
    catalog.add(
        ASH,
        material(
            powder()
                .air_drag(4.5)
                .ground_friction(55.0)
                .topple(31.0, 120.0)
                .deflect(0.4)
                .cohesion(0.05),
        )
        .density(550.0)
        .colors([
            [86, 82, 80, 255],
            [74, 70, 68, 255],
            [98, 94, 92, 255],
            [64, 60, 60, 255],
        ])
        .hardness(0.02)
        .tag(Tag::Dissolvable),
    );
    catalog.add(
        CHARCOAL,
        material(
            powder()
                .air_drag(3.2)
                .ground_friction(76.0)
                .topple(22.0, 86.0)
                .deflect(0.3)
                .cohesion(0.08),
        )
        .density(900.0)
        .colors([
            [42, 38, 38, 255],
            [35, 32, 32, 255],
            [52, 47, 47, 255],
            [29, 26, 26, 255],
        ])
        .hardness(0.08)
        .friction(0.65)
        .tag(Tag::Dissolvable)
        .flammable(
            flammable()
                .ignite(2.5)
                .sealed_burn(0.3)
                .rate(0.02)
                .emit(6.0)
                .colors([
                    [255, 128, 40, 255],
                    [226, 96, 24, 255],
                    [255, 168, 66, 255],
                    [196, 72, 14, 255],
                ])
                .residue(ASH, 0.1)
                .burnout(SMOKE)
                .damage(9.0),
        ),
    );
}
