use fallingsand_core::Phase::*;
use fallingsand_core::Tag::*;
use fallingsand_core::Tags;

materials! {
    base: crate::material::base::special;

    AIR = Material {
        phase: Empty,
        density: 1.2,
        colors: &[[0, 0, 0, 0]],
    },
    FLESH = Material {
        phase: Solid,
        density: 1050.0,
        colors: &[
            [155, 111, 154, 255],
            [39, 33, 37, 255],
            [127, 84, 118, 255],
            [89, 67, 84, 255],
            [209, 155, 61, 255],
            [219, 192, 103, 255],
            [245, 222, 145, 255],
        ],
        surface_grip: 0.8,
        tags: Tags::new(&[Player]),
    },
}
