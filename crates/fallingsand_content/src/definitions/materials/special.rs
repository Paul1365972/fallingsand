use crate::{Catalog, MaterialKey, Tag, empty, material, solid};

pub const AIR: MaterialKey = MaterialKey::new("AIR");
pub const FLESH: MaterialKey = MaterialKey::new("FLESH");

pub fn define(catalog: &mut Catalog) {
    catalog.add(AIR, material(empty()).density(1.2).colors([[0, 0, 0, 0]]));
    catalog.add(
        FLESH,
        material(solid())
            .density(1050.0)
            .colors([
                [155, 111, 154, 255],
                [39, 33, 37, 255],
                [127, 84, 118, 255],
                [89, 67, 84, 255],
                [209, 155, 61, 255],
                [219, 192, 103, 255],
                [245, 222, 145, 255],
            ])
            .surface_grip(0.8)
            .tag(Tag::Player),
    );
}
