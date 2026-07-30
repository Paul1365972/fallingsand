use super::fire::SMOKE;
use crate::{Catalog, MaterialKey, Tag, emission, flammable, liquid, material};

pub const WATER: MaterialKey = MaterialKey::new("WATER");
pub const BRINE: MaterialKey = MaterialKey::new("BRINE");
pub const OIL: MaterialKey = MaterialKey::new("OIL");
pub const TAR: MaterialKey = MaterialKey::new("TAR");
pub const RESIN: MaterialKey = MaterialKey::new("RESIN");
pub const LAVA: MaterialKey = MaterialKey::new("LAVA");
pub const ACID: MaterialKey = MaterialKey::new("ACID");
pub const MOLTEN_IRON: MaterialKey = MaterialKey::new("MOLTEN_IRON");
pub const MOLTEN_GLASS: MaterialKey = MaterialKey::new("MOLTEN_GLASS");
pub const ICHOR: MaterialKey = MaterialKey::new("ICHOR");
pub const TOXIC_SLUDGE: MaterialKey = MaterialKey::new("TOXIC_SLUDGE");
pub const SLIME: MaterialKey = MaterialKey::new("SLIME");
pub const ALCOHOL: MaterialKey = MaterialKey::new("ALCOHOL");
pub const BLOOD: MaterialKey = MaterialKey::new("BLOOD");
pub const CEMENT: MaterialKey = MaterialKey::new("CEMENT");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        WATER,
        material(liquid().drag(0.48).impact(0.72))
            .density(1000.0)
            .restitution(0.35)
            .colors([[44, 96, 200, 190], [40, 90, 192, 190], [48, 102, 208, 190]]),
    );
    catalog.add(
        BRINE,
        material(liquid().drag(0.55).impact(0.7))
            .density(1150.0)
            .restitution(0.3)
            .colors([
                [66, 118, 156, 195],
                [58, 108, 146, 195],
                [76, 130, 168, 195],
            ]),
    );
    catalog.add(
        OIL,
        material(liquid().drag(2.45).impact(0.55).flow_rate(30.0))
            .density(850.0)
            .colors([[74, 62, 36, 215], [66, 54, 30, 215], [84, 72, 44, 215]])
            .flammable(
                flammable()
                    .ignite(30.0)
                    .sealed_burn(0.0)
                    .rate(0.4)
                    .emit(18.0)
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
        TAR,
        material(liquid().drag(9.0).impact(0.3).flow_rate(2.0))
            .density(1050.0)
            .colors([[28, 26, 24, 240], [22, 21, 19, 240], [36, 33, 31, 240]])
            .traction(0.2)
            .flammable(
                flammable()
                    .ignite(6.0)
                    .sealed_burn(0.05)
                    .rate(0.08)
                    .emit(12.0)
                    .colors([
                        [240, 128, 40, 255],
                        [208, 96, 24, 255],
                        [255, 160, 60, 255],
                        [176, 72, 16, 255],
                    ])
                    .burnout(SMOKE)
                    .damage(9.0),
            ),
    );
    catalog.add(
        RESIN,
        material(liquid().drag(7.0).impact(0.35).flow_rate(4.0))
            .density(1020.0)
            .colors([
                [206, 132, 44, 220],
                [188, 116, 36, 220],
                [222, 150, 58, 220],
            ])
            .emission(emission([206, 132, 44]).intensity(0.12))
            .flammable(
                flammable()
                    .ignite(18.0)
                    .sealed_burn(0.0)
                    .rate(0.5)
                    .emit(16.0)
                    .colors([
                        [255, 190, 70, 255],
                        [248, 150, 40, 255],
                        [255, 220, 110, 255],
                        [226, 118, 24, 255],
                    ])
                    .burnout(SMOKE)
                    .damage(8.0),
            ),
    );
    catalog.add(
        LAVA,
        material(liquid().drag(6.0).impact(0.5).flow_rate(8.0))
            .density(2800.0)
            .colors([
                [255, 96, 24, 255],
                [240, 80, 16, 255],
                [255, 128, 32, 255],
                [224, 64, 8, 255],
            ])
            .contact_damage(30.0)
            .tags([Tag::Hot])
            .emission(emission([255, 96, 24]).intensity(1.0)),
    );
    catalog.add(
        ACID,
        material(liquid().drag(0.8).impact(0.72).flow_rate(60.0))
            .density(1200.0)
            .colors([
                [128, 220, 56, 210],
                [116, 208, 48, 210],
                [142, 232, 68, 210],
            ])
            .contact_damage(12.0),
    );
    catalog.add(
        MOLTEN_IRON,
        material(liquid().drag(4.0).impact(0.45).flow_rate(14.0))
            .density(6900.0)
            .colors([
                [255, 214, 150, 255],
                [255, 186, 108, 255],
                [255, 238, 196, 255],
                [244, 160, 74, 255],
            ])
            .contact_damage(34.0)
            .tags([Tag::Hot])
            .emission(emission([255, 200, 130]).intensity(1.3)),
    );
    catalog.add(
        MOLTEN_GLASS,
        material(liquid().drag(7.5).impact(0.4).flow_rate(6.0))
            .density(2400.0)
            .colors([
                [255, 176, 96, 235],
                [246, 156, 78, 235],
                [255, 202, 130, 235],
            ])
            .contact_damage(26.0)
            .tags([Tag::Hot])
            .emission(emission([255, 170, 90]).intensity(0.9)),
    );
    catalog.add(
        TOXIC_SLUDGE,
        material(liquid().drag(3.6).impact(0.5).flow_rate(12.0))
            .density(1320.0)
            .colors([
                [124, 176, 44, 225],
                [110, 160, 36, 225],
                [140, 194, 58, 225],
            ])
            .contact_damage(7.0)
            .emission(emission([116, 168, 40]).intensity(0.3))
            .tags([Tag::Suffocating]),
    );
    catalog.add(
        SLIME,
        material(liquid().drag(11.0).impact(0.24).flow_rate(3.0))
            .density(1240.0)
            .colors([
                [168, 196, 148, 220],
                [152, 180, 132, 220],
                [186, 212, 166, 220],
            ])
            .traction(0.14),
    );
    catalog.add(
        ALCOHOL,
        material(liquid().drag(0.42).impact(0.74).flow_rate(70.0))
            .density(800.0)
            .colors([
                [214, 206, 176, 180],
                [200, 192, 162, 180],
                [228, 220, 192, 180],
            ])
            .flammable(
                flammable()
                    .ignite(48.0)
                    .sealed_burn(0.0)
                    .rate(3.0)
                    .emit(22.0)
                    .colors([
                        [128, 176, 255, 255],
                        [96, 148, 248, 255],
                        [176, 208, 255, 255],
                        [72, 120, 226, 255],
                    ])
                    .burnout(SMOKE)
                    .damage(10.0),
            ),
    );
    catalog.add(
        BLOOD,
        material(liquid().drag(1.4).impact(0.62).flow_rate(26.0))
            .density(1060.0)
            .colors([[138, 26, 30, 225], [122, 20, 24, 225], [158, 36, 40, 225]]),
    );
    catalog.add(
        CEMENT,
        material(liquid().drag(8.0).impact(0.3).flow_rate(5.0))
            .density(2100.0)
            .colors([
                [154, 150, 142, 235],
                [140, 136, 129, 235],
                [170, 166, 157, 235],
            ])
            .traction(0.3),
    );
    catalog.add(
        ICHOR,
        material(liquid().drag(3.2).impact(0.5).flow_rate(18.0))
            .density(1100.0)
            .colors([
                [186, 224, 108, 220],
                [166, 208, 92, 220],
                [206, 240, 130, 220],
            ])
            .contact_damage(3.0)
            .emission(emission([176, 220, 100]).intensity(0.28))
            .flammable(
                flammable()
                    .ignite(10.0)
                    .sealed_burn(0.0)
                    .rate(1.2)
                    .emit(14.0)
                    .burnout(SMOKE)
                    .damage(9.0),
            ),
    );
}
