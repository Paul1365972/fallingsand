FIRE = Material {
    phase: Gas {
        drag: 6.0,
        turbulence: 52.0,
    },
    density: 0.3,
    colors: [
        [255, 160, 32, 255],
        [255, 120, 16, 255],
        [255, 200, 64, 255],
        [232, 88, 8, 255],
    ],
    ember: true,
    burn_rate: 6.3,
    burnout_into: SMOKE,
    contact_damage: 8.0,
    tags: [Hot, Emissive],
},
SMOKE = Material {
    phase: Gas {
        drag: 7.0,
        cohesion: 0.3,
        turbulence: 90.0,
    },
    density: 0.4,
    colors: [
        [60, 58, 56, 140],
        [52, 50, 48, 120],
        [70, 68, 66, 150],
    ],
},
ASH = Material {
    phase: Powder {
        drag: 4.5,
        friction: 55.0,
        repose: 31.0,
        redirect_keep: 0.4,
    },
    density: 550.0,
    colors: [
        [86, 82, 80, 255],
        [74, 70, 68, 255],
        [98, 94, 92, 255],
        [64, 60, 60, 255],
    ],
    hardness: 0.02,
    tags: [Dissolvable],
},
