use super::fire::{ASH, SMOKE};
use crate::{Catalog, Tag, burning, emission, inherit, material, material_keys, solid};

material_keys! { WOOD, MOSS, LEAVES, PLANKS, MUSHROOM_STEM, GLOWSHROOM }

pub fn define(catalog: &mut Catalog) {
    catalog.add(
        WOOD,
        material(solid().rigid())
            .density(700.0)
            .colors([
                [133, 94, 66, 255],
                [120, 82, 56, 255],
                [145, 104, 76, 255],
                [110, 76, 52, 255],
            ])
            .hardness(0.35)
            .restitution(0.3)
            .tag(Tag::Dissolvable)
            .burning(
                burning()
                    .ignite(1.0)
                    .smoulder(0.05)
                    .rate(0.25)
                    .emit(10.0)
                    .colors([
                        [255, 150, 40, 255],
                        [240, 116, 24, 255],
                        [255, 184, 64, 255],
                        [208, 92, 16, 255],
                    ])
                    .residue(ASH, 0.35)
                    .burnout(SMOKE)
                    .damage(8.0),
            ),
    );
    catalog.add(
        MOSS,
        material(solid())
            .density(500.0)
            .colors([
                [66, 112, 52, 255],
                [58, 102, 46, 255],
                [74, 122, 58, 255],
                [52, 94, 42, 255],
            ])
            .hardness(0.05)
            .restitution(0.05)
            .tag(Tag::Dissolvable)
            .burning(
                burning()
                    .ignite(3.0)
                    .rate(5.0)
                    .emit(18.0)
                    .residue(ASH, 0.3)
                    .burnout(SMOKE)
                    .damage(7.0),
            ),
    );
    catalog.add(
        LEAVES,
        inherit(MOSS)
            .phase(solid().rigid())
            .density(350.0)
            .colors([
                [68, 138, 58, 255],
                [58, 126, 50, 255],
                [78, 150, 66, 255],
                [50, 116, 44, 255],
            ])
            .hardness(0.03)
            .burning(
                burning()
                    .ignite(3.0)
                    .rate(2.5)
                    .emit(18.0)
                    .residue(ASH, 0.3)
                    .burnout(SMOKE)
                    .damage(7.0),
            ),
    );
    catalog.add(
        PLANKS,
        inherit(WOOD)
            .phase(solid().rigid())
            .density(600.0)
            .colors([
                [172, 132, 86, 255],
                [162, 122, 78, 255],
                [182, 142, 94, 255],
                [152, 114, 72, 255],
            ])
            .hardness(0.3),
    );
    catalog.add(
        MUSHROOM_STEM,
        inherit(MOSS)
            .phase(solid().rigid())
            .density(400.0)
            .colors([
                [216, 206, 186, 255],
                [204, 194, 174, 255],
                [228, 218, 198, 255],
                [192, 182, 164, 255],
            ])
            .hardness(0.1)
            .restitution(0.3)
            .surface_bounce(0.6),
    );
    catalog.add(
        GLOWSHROOM,
        inherit(MUSHROOM_STEM)
            .colors([
                [90, 220, 190, 255],
                [70, 200, 170, 255],
                [110, 240, 210, 255],
                [60, 180, 155, 255],
            ])
            .tags([Tag::Dissolvable])
            .emission(emission([90, 220, 190]).intensity(0.6)),
    );
}
