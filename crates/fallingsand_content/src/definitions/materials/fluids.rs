use super::fire::SMOKE;
use crate::{Catalog, Tag, burning, emission, gas, liquid, material, material_keys};

material_keys! { WATER, STEAM, OIL, LAVA, ACID }

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        WATER,
        material(
            liquid()
                .drag(2.5)
                .friction(1.2)
                .redirect_keep(0.98)
                .cohesion(8.0),
        )
        .density(1000.0)
        .colors([[44, 96, 200, 190], [40, 90, 192, 190], [48, 102, 208, 190]]),
    );
    catalog.add(
        STEAM,
        material(gas().drag(6.0).cohesion(0.4).turbulence(39.0))
            .density(0.6)
            .colors([
                [200, 200, 210, 90],
                [190, 190, 200, 80],
                [210, 210, 220, 100],
            ]),
    );
    catalog.add(
        OIL,
        material(
            liquid()
                .drag(3.0)
                .friction(6.3)
                .redirect_keep(0.9)
                .cohesion(5.0),
        )
        .density(850.0)
        .colors([[74, 62, 36, 215], [66, 54, 30, 215], [84, 72, 44, 215]])
        .burning(
            burning()
                .ignite(3.0)
                .rate(0.5)
                .emit(16.0)
                .colors([
                    [255, 168, 48, 255],
                    [255, 128, 28, 255],
                    [255, 200, 72, 255],
                    [232, 100, 18, 255],
                ])
                .burnout(SMOKE)
                .damage(8.0),
        ),
    );
    catalog.add(
        LAVA,
        material(
            liquid()
                .drag(6.0)
                .friction(42.0)
                .redirect_keep(0.5)
                .cohesion(1.5)
                .flow_rate(15.0),
        )
        .density(2800.0)
        .colors([
            [255, 96, 24, 255],
            [240, 80, 16, 255],
            [255, 128, 32, 255],
            [224, 64, 8, 255],
        ])
        .contact_damage(30.0)
        .tags([Tag::Hot])
        .emission(emission([255, 96, 24]).intensity(2.0)),
    );
    catalog.add(
        ACID,
        material(
            liquid()
                .drag(2.8)
                .friction(3.1)
                .redirect_keep(0.95)
                .cohesion(6.0),
        )
        .density(1200.0)
        .colors([
            [128, 220, 56, 210],
            [116, 208, 48, 210],
            [142, 232, 68, 210],
        ])
        .contact_damage(12.0),
    );
}
