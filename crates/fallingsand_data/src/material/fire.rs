use fallingsand_core::Phase::*;
use fallingsand_core::Tag::*;
use fallingsand_core::Tags;

materials! {
    base: crate::material::base::fire;

    FIRE = Material {
        phase: Fire,
        density: 0.3,
        colors: &[
            [255, 160, 32, 255],
            [255, 120, 16, 255],
            [255, 200, 64, 255],
            [232, 88, 8, 255],
        ],
        drag: 6.0,
        turbulence: 52.0,
        decay_rate: 6.3,
        decay_into: Some(SMOKE),
        contact_damage: 8.0,
        tags: Tags::new(&[Hot, Emissive]),
    },
    SMOKE = Material {
        phase: Gas,
        density: 0.4,
        colors: &[
            [60, 58, 56, 140],
            [52, 50, 48, 120],
            [70, 68, 66, 150],
        ],
        drag: 7.0,
        cohesion: 0.3,
        turbulence: 90.0,
        decay_rate: 0.36,
    },
    ASH = Material {
        phase: Powder,
        density: 550.0,
        colors: &[
            [86, 82, 80, 255],
            [74, 70, 68, 255],
            [98, 94, 92, 255],
            [64, 60, 60, 255],
        ],
        drag: 4.5,
        friction: 55.0,
        repose: 31.0,
        redirect_keep: 0.4,
        hardness: 0.02,
        tags: Tags::new(&[Dissolvable]),
    },
}
