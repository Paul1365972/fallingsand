use crate::{Catalog, MaterialKey, Tag, empty, material, solid};

pub const AIR: MaterialKey = MaterialKey::new("AIR");
pub const BODY: MaterialKey = MaterialKey::new("BODY");
pub const CORPSE: MaterialKey = MaterialKey::new("CORPSE");
pub const RUBBER: MaterialKey = MaterialKey::new("RUBBER");
pub const FROG: MaterialKey = MaterialKey::new("FROG");

pub fn define(catalog: &mut Catalog) {
    catalog.add(AIR, material(empty()).density(1.2).colors([[0, 0, 0, 0]]));
    catalog.add(
        BODY,
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
            .traction(0.8)
            .friction(0.8)
            .bond_group("flesh")
            .tag(Tag::Body),
    );
    catalog.add(
        CORPSE,
        material(solid())
            .density(1050.0)
            .colors([
                [116, 92, 116, 255],
                [32, 28, 32, 255],
                [96, 70, 92, 255],
                [70, 55, 68, 255],
                [148, 118, 62, 255],
                [160, 146, 92, 255],
                [182, 172, 128, 255],
            ])
            .hardness(0.05)
            .traction(0.3)
            .friction(0.25)
            .bond_group("corpse")
            .tag(Tag::Worthless),
    );
    catalog.add(
        RUBBER,
        material(solid())
            .density(400.0)
            .colors([
                [198, 54, 58, 255],
                [236, 118, 106, 255],
                [138, 30, 44, 255],
                [238, 230, 218, 255],
            ])
            .traction(0.9)
            .friction(0.9)
            .restitution(0.85)
            .bond_group("rubber")
            .tag(Tag::Body),
    );
    catalog.add(
        FROG,
        material(solid())
            .density(980.0)
            .colors([
                [88, 154, 72, 255],
                [56, 108, 48, 255],
                [198, 214, 152, 255],
                [26, 30, 24, 255],
            ])
            .traction(0.8)
            .friction(0.7)
            .bond_group("frog")
            .tag(Tag::Body),
    );
}
