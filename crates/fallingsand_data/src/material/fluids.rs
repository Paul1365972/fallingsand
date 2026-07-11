use fallingsand_core::Phase::*;
use fallingsand_core::Tag::*;
use fallingsand_core::Tags;

materials! {
    base: crate::material::base::fluids;

    WATER = Material {
        phase: Liquid,
        density: 1000.0,
        colors: &[
            [44, 96, 200, 190],
            [40, 90, 192, 190],
            [48, 102, 208, 190],
        ],
        drag: 2.5,
        friction: 1.2,
        redirect_keep: 0.98,
        cohesion: 8.0,
    },
    STEAM = Material {
        phase: Gas,
        density: 0.6,
        colors: &[
            [200, 200, 210, 90],
            [190, 190, 200, 80],
            [210, 210, 220, 100],
        ],
        drag: 6.0,
        cohesion: 0.4,
        turbulence: 39.0,
        decay_rate: 0.1,
        decay_into: Some(WATER),
    },
    OIL = Material {
        phase: Liquid,
        density: 850.0,
        colors: &[
            [74, 62, 36, 215],
            [66, 54, 30, 215],
            [84, 72, 44, 215],
        ],
        drag: 3.0,
        friction: 6.3,
        redirect_keep: 0.9,
        cohesion: 5.0,
        flammability: 3.0,
        burn_rate: 0.5,
        burn_emit: 16.0,
        burn_damage: 8.0,
    },
    LAVA = Material {
        phase: Liquid,
        density: 2800.0,
        colors: &[
            [255, 96, 24, 255],
            [240, 80, 16, 255],
            [255, 128, 32, 255],
            [224, 64, 8, 255],
        ],
        drag: 6.0,
        friction: 42.0,
        redirect_keep: 0.5,
        cohesion: 1.5,
        flow_rate: 15.0,
        contact_damage: 30.0,
        tags: Tags::new(&[Hot, Emissive]),
    },
    ACID = Material {
        phase: Liquid,
        density: 1200.0,
        colors: &[
            [128, 220, 56, 210],
            [116, 208, 48, 210],
            [142, 232, 68, 210],
        ],
        drag: 2.8,
        friction: 3.1,
        redirect_keep: 0.95,
        cohesion: 6.0,
        contact_damage: 12.0,
    },
}
