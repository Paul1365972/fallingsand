use crate::material::ASH;
use fallingsand_core::Phase::*;
use fallingsand_core::Tag::*;
use fallingsand_core::Tags;

materials! {
    base: crate::material::base::ores;

    COAL = Material {
        phase: Solid,
        density: 1450.0,
        colors: &[
            [52, 50, 52, 255],
            [44, 42, 44, 255],
            [62, 60, 62, 255],
            [38, 36, 40, 255],
        ],
        rigid_capable: true,
        hardness: 0.5,
        restitution: 0.1,
        tags: Tags::new(&[Dissolvable]),
        flammability: 0.6,
        burn_rate: 0.04,
        burn_emit: 5.0,
        smoulder: 0.2,
        residue_into: Some(ASH),
        residue_chance: 0.05,
        burn_damage: 8.0,
    },
    IRON_ORE = Material {
        phase: Solid,
        density: 3200.0,
        colors: &[
            [146, 116, 96, 255],
            [132, 104, 86, 255],
            [158, 126, 104, 255],
            [120, 96, 82, 255],
        ],
        rigid_capable: true,
        hardness: 1.4,
        restitution: 0.1,
        tags: Tags::new(&[Dissolvable]),
    },
    GOLD_ORE = Material {
        phase: Solid,
        density: 3600.0,
        colors: &[
            [196, 164, 62, 255],
            [180, 148, 52, 255],
            [212, 180, 76, 255],
            [166, 136, 46, 255],
        ],
        rigid_capable: true,
        hardness: 1.6,
        restitution: 0.1,
        tags: Tags::new(&[Dissolvable]),
    },
    CRYSTAL = Material {
        phase: Solid,
        density: 2650.0,
        colors: &[
            [150, 220, 255, 255],
            [120, 190, 250, 255],
            [180, 240, 255, 255],
            [100, 170, 240, 255],
        ],
        rigid_capable: true,
        hardness: 1.8,
        restitution: 0.15,
        tags: Tags::new(&[Emissive, Dissolvable]),
    },
}
