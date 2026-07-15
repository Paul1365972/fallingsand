use crate::{Catalog, Tag, burning, emission, gas, material, material_keys, powder};

material_keys! { FIRE, SMOKE, ASH }

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        FIRE,
        material(gas().drag(5.5).turbulence(80.0).redirect_keep(0.4))
            .density(0.3)
            .colors([
                [255, 160, 32, 255],
                [255, 120, 16, 255],
                [255, 200, 64, 255],
                [232, 88, 8, 255],
            ])
            .burning(burning().rate(6.3).burnout(SMOKE))
            .contact_damage(8.0)
            .tags([Tag::Hot])
            .emission(emission([255, 140, 32]).intensity(3.5).flicker(0.5)),
    );
    catalog.add(
        SMOKE,
        material(gas().drag(7.0).cohesion(0.3).turbulence(90.0))
            .density(0.4)
            .colors([[60, 58, 56, 140], [52, 50, 48, 120], [70, 68, 66, 150]]),
    );
    catalog.add(
        ASH,
        material(
            powder()
                .drag(4.5)
                .friction(55.0)
                .repose(31.0)
                .redirect_keep(0.4),
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
}
