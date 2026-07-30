use super::fire::SMOKE;
use crate::{Catalog, MaterialKey, Tag, emission, flammable, gas, material};

pub const STEAM: MaterialKey = MaterialKey::new("STEAM");
pub const FIREDAMP: MaterialKey = MaterialKey::new("FIREDAMP");
pub const CHOKEDAMP: MaterialKey = MaterialKey::new("CHOKEDAMP");
pub const SPOREGAS: MaterialKey = MaterialKey::new("SPOREGAS");
pub const TOXIC_GAS: MaterialKey = MaterialKey::new("TOXIC_GAS");
pub const VOIDMIST: MaterialKey = MaterialKey::new("VOIDMIST");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        STEAM,
        material(gas().air_drag(6.0).turbulence(39.0))
            .density(0.6)
            .colors([
                [200, 200, 210, 90],
                [190, 190, 200, 80],
                [210, 210, 220, 100],
            ]),
    );

    catalog.add(
        FIREDAMP,
        material(gas().air_drag(5.0).turbulence(50.0))
            .density(0.55)
            .colors([
                [150, 168, 148, 70],
                [138, 156, 138, 60],
                [162, 180, 160, 80],
            ])
            .flammable(
                flammable()
                    .ignite(60.0)
                    .sealed_burn(0.0)
                    .rate(30.0)
                    .emit(40.0)
                    .colors([
                        [255, 214, 120, 255],
                        [255, 180, 72, 255],
                        [255, 238, 176, 255],
                        [244, 150, 48, 255],
                    ])
                    .burnout(SMOKE)
                    .damage(14.0),
            ),
    );

    catalog.add(
        CHOKEDAMP,
        material(gas().air_drag(8.0).turbulence(16.0))
            .density(1.9)
            .colors([
                [122, 118, 104, 90],
                [110, 107, 94, 78],
                [134, 130, 116, 102],
            ])
            .tags([Tag::Suffocating]),
    );

    catalog.add(
        SPOREGAS,
        material(gas().air_drag(7.0).turbulence(34.0))
            .density(0.95)
            .colors([
                [140, 196, 156, 120],
                [126, 180, 142, 105],
                [156, 212, 172, 135],
            ])
            .tags([Tag::Suffocating])
            .flammable(
                flammable()
                    .ignite(45.0)
                    .sealed_burn(0.0)
                    .rate(24.0)
                    .emit(30.0)
                    .colors([
                        [214, 255, 168, 255],
                        [180, 240, 130, 255],
                        [238, 255, 200, 255],
                        [150, 214, 100, 255],
                    ])
                    .burnout(SMOKE)
                    .damage(10.0),
            ),
    );

    catalog.add(
        TOXIC_GAS,
        material(gas().air_drag(8.5).turbulence(20.0))
            .density(2.3)
            .colors([
                [150, 200, 78, 120],
                [134, 184, 66, 104],
                [170, 220, 96, 138],
            ])
            .contact_damage(6.0)
            .emission(emission([140, 190, 70]).intensity(0.22))
            .tags([Tag::Suffocating]),
    );

    catalog.add(
        VOIDMIST,
        material(gas().air_drag(9.5).turbulence(8.0))
            .density(1.5)
            .colors([[26, 24, 34, 190], [21, 20, 29, 175], [32, 30, 40, 205]])
            .tags([Tag::Suffocating]),
    );
}
