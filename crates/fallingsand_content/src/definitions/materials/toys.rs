use super::fire::SMOKE;
use crate::{Catalog, MaterialKey, flammable, material, solid};

pub const BALLOON: MaterialKey = MaterialKey::new("BALLOON");
pub const BALLOON_STRING: MaterialKey = MaterialKey::new("BALLOON_STRING");

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        BALLOON,
        material(solid())
            .density(0.5)
            .colors([
                [214, 62, 76, 255],
                [244, 130, 138, 255],
                [162, 42, 58, 255],
                [230, 92, 102, 255],
            ])
            .hardness(0.02)
            .restitution(0.25)
            .friction(0.2)
            .bond_group("balloon")
            .flammable(
                flammable()
                    .ignite(6.0)
                    .rate(6.0)
                    .emit(4.0)
                    .burnout(SMOKE)
                    .damage(1.0),
            ),
    );
    catalog.add(
        BALLOON_STRING,
        material(solid())
            .density(2.0)
            .colors([
                [224, 216, 196, 255],
                [206, 196, 174, 255],
                [238, 230, 212, 255],
                [192, 182, 162, 255],
            ])
            .hardness(0.02)
            .restitution(0.05)
            .friction(0.4)
            .bond_group("balloon")
            .flammable(
                flammable()
                    .ignite(5.0)
                    .rate(5.0)
                    .emit(3.0)
                    .burnout(SMOKE)
                    .damage(1.0),
            ),
    );
}
