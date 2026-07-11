WOOD = Material {
    phase: Solid { rigid_capable: true },
    density: 700.0,
    colors: [
        [133, 94, 66, 255],
        [120, 82, 56, 255],
        [145, 104, 76, 255],
        [110, 76, 52, 255],
    ],
    hardness: 0.35,
    restitution: 0.3,
    tags: [Dissolvable],
    burn_variant: Burning {
        ignite: 1.0,
        smoulder: 0.05,
        rate: 0.35,
        emit: 10.0,
        colors: [
            [255, 150, 40, 255],
            [240, 116, 24, 255],
            [255, 184, 64, 255],
            [208, 92, 16, 255],
        ],
        residue: ASH,
        residue_chance: 0.35,
        burnout: SMOKE,
        damage: 8.0,
    },
},
MOSS = Material {
    phase: Solid,
    density: 500.0,
    colors: [
        [66, 112, 52, 255],
        [58, 102, 46, 255],
        [74, 122, 58, 255],
        [52, 94, 42, 255],
    ],
    hardness: 0.05,
    restitution: 0.05,
    tags: [Dissolvable],
    burn_variant: Burning {
        ignite: 3.0,
        rate: 5.0,
        emit: 18.0,
        residue: ASH,
        residue_chance: 0.3,
        burnout: SMOKE,
        damage: 7.0,
    },
},
LEAVES = Material {
    phase: Solid { rigid_capable: true },
    density: 350.0,
    colors: [
        [68, 138, 58, 255],
        [58, 126, 50, 255],
        [78, 150, 66, 255],
        [50, 116, 44, 255],
    ],
    hardness: 0.03,
    ..MOSS
},
PLANKS = Material {
    phase: Solid { rigid_capable: true },
    density: 600.0,
    colors: [
        [172, 132, 86, 255],
        [162, 122, 78, 255],
        [182, 142, 94, 255],
        [152, 114, 72, 255],
    ],
    hardness: 0.3,
    ..WOOD
},
MUSHROOM_STEM = Material {
    phase: Solid { rigid_capable: true },
    density: 400.0,
    colors: [
        [216, 206, 186, 255],
        [204, 194, 174, 255],
        [228, 218, 198, 255],
        [192, 182, 164, 255],
    ],
    hardness: 0.1,
    restitution: 0.3,
    surface_bounce: 0.6,
    ..MOSS
},
GLOWSHROOM = Material {
    colors: [
        [90, 220, 190, 255],
        [70, 200, 170, 255],
        [110, 240, 210, 255],
        [60, 180, 155, 255],
    ],
    tags: [Dissolvable, Emissive],
    ..MUSHROOM_STEM
},
