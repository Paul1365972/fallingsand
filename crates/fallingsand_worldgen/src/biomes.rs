use crate::scale::len;
use crate::terrain::Params;
use fallingsand_core::MaterialId;
use fallingsand_core::content::material;

const ANY: (f32, f32) = (0.0, 1.0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lithology {
    Soil,
    Carbonate,
    Clastic,
    Crystalline,
    Igneous,
    Frozen,
    Fungal,
    Flesh,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tree {
    None,
    Broadleaf,
    Resinpine,
    Redwood,
    Mangrove,
    Hawthorn,
    DwarfWillow,
    Snag,
    Sporecap,
}

#[derive(Clone, Copy)]
pub struct Skin {
    pub cover: MaterialId,
    pub cover_depth: i32,
    pub soil: MaterialId,
    pub soil_depth: i32,
    pub subsoil: MaterialId,
    pub subsoil_depth: i32,
    pub tree: Tree,
    pub tree_spacing: i32,
    pub ground_cover: Option<MaterialId>,
    pub cover_chance: f32,
    pub submerged: bool,
    pub wade: i32,
    pub boulder_spacing: i32,
    pub boulder: MaterialId,
}

#[derive(Clone, Copy)]
pub struct SubBiome {
    pub name: &'static str,
    pub depth: (f32, f32),
    pub heat: (f32, f32),
    pub wet: (f32, f32),
    pub weird: (f32, f32),
    pub variant: (f32, f32),
    pub priority: u8,

    pub lithology: Lithology,
    pub stone: MaterialId,
    pub streak: MaterialId,
    pub sediment: MaterialId,
    pub fluid: Option<MaterialId>,
    pub fluid_level: i32,
    pub gas: Option<MaterialId>,
    pub gas_chance: f32,

    pub solidity: f32,
    pub worms: f32,
    pub chambers: f32,
    pub galleries: f32,

    pub prize: MaterialId,
    pub skin: Option<Skin>,
}

pub struct Biome {
    pub name: &'static str,
    pub land: (f32, f32),
    pub relief: (f32, f32),
    pub rock: (f32, f32),
    pub heat: (f32, f32),
    pub wet: (f32, f32),
    pub weird: (f32, f32),
    pub depth: (f32, f32),
    pub daylight: bool,
    pub priority: u8,
    pub members: Vec<SubBiome>,
}

impl SubBiome {
    pub fn hosts(&self, present: MaterialId) -> bool {
        present == self.stone
            || present == self.streak
            || matches!(self.skin, Some(skin) if present == skin.subsoil)
    }
}

const fn turf_skin() -> Skin {
    Skin {
        cover: material::TURF,
        cover_depth: 1,
        soil: material::DIRT,
        soil_depth: 10,
        subsoil: material::CLAY,
        subsoil_depth: 40,
        tree: Tree::None,
        tree_spacing: 0,
        ground_cover: Some(material::GRASS_BLADE),
        cover_chance: 0.35,
        submerged: false,
        wade: 0,
        boulder_spacing: 600,
        boulder: material::GRANITE,
    }
}

const fn buried() -> SubBiome {
    SubBiome {
        name: "",
        depth: ANY,
        heat: ANY,
        wet: ANY,
        weird: ANY,
        variant: ANY,
        priority: 0,
        lithology: Lithology::Clastic,
        stone: material::GRANITE,
        streak: material::GRAVEL,
        sediment: material::GRAVEL,
        fluid: None,
        fluid_level: 0,
        gas: None,
        gas_chance: 0.0,
        solidity: 0.28,
        worms: 1.05,
        chambers: 1.0,
        galleries: 0.8,
        prize: material::IRON_ORE,
        skin: None,
    }
}

const fn daylight() -> SubBiome {
    SubBiome {
        lithology: Lithology::Soil,
        stone: material::LIMESTONE,
        streak: material::CLAY,
        sediment: material::GRAVEL,
        solidity: 0.52,
        worms: 0.5,
        chambers: 0.3,
        galleries: 0.2,
        prize: material::NATIVE_COPPER,
        skin: Some(turf_skin()),
        ..buried()
    }
}

fn meadow() -> Biome {
    Biome {
        name: "Meadowveldt",
        land: (0.30, 1.0),
        relief: ANY,
        rock: ANY,
        heat: ANY,
        wet: ANY,
        weird: ANY,
        depth: (0.0, 0.05),
        daylight: true,
        priority: 0,
        members: vec![
            SubBiome {
                name: "Meadowveldt",
                skin: Some(Skin {
                    tree: Tree::Broadleaf,
                    tree_spacing: 130,
                    ground_cover: Some(material::WILDFLOWER),
                    ..turf_skin()
                }),
                ..daylight()
            },
            SubBiome {
                name: "Hedgerow Downs",
                variant: (0.50, 1.0),
                priority: 1,
                skin: Some(Skin {
                    soil_depth: 16,
                    subsoil: material::LIMESTONE,
                    subsoil_depth: 44,
                    tree: Tree::Hawthorn,
                    tree_spacing: 90,
                    ..turf_skin()
                }),
                ..daylight()
            },
        ],
    }
}

fn pinewood() -> Biome {
    Biome {
        name: "Pinewood",
        land: (0.32, 1.0),
        relief: ANY,
        rock: ANY,
        heat: (0.16, 0.62),
        wet: (0.50, 0.94),
        weird: (0.0, 0.62),
        depth: (0.0, 0.05),
        daylight: true,
        priority: 1,
        members: vec![
            SubBiome {
                name: "Resinpine Wood",
                lithology: Lithology::Crystalline,
                stone: material::GRANITE,
                streak: material::QUARTZ,
                skin: Some(Skin {
                    cover: material::MOSS,
                    cover_depth: 2,
                    soil: material::PEAT,
                    soil_depth: 24,
                    subsoil: material::GRANITE,
                    subsoil_depth: 50,
                    tree: Tree::Resinpine,
                    tree_spacing: 34,
                    ground_cover: Some(material::LICHEN),
                    cover_chance: 0.22,
                    ..turf_skin()
                }),
                ..daylight()
            },
            SubBiome {
                name: "Redwood Colonnade",
                variant: (0.54, 1.0),
                priority: 1,
                lithology: Lithology::Crystalline,
                stone: material::GRANITE,
                streak: material::QUARTZ,
                skin: Some(Skin {
                    cover: material::MOSS,
                    cover_depth: 6,
                    soil: material::DIRT,
                    soil_depth: 40,
                    subsoil: material::GRANITE,
                    subsoil_depth: 20,
                    tree: Tree::Redwood,
                    tree_spacing: 70,
                    ground_cover: Some(material::MOSS),
                    cover_chance: 0.3,
                    ..turf_skin()
                }),
                ..daylight()
            },
        ],
    }
}

fn snowfield() -> Biome {
    Biome {
        name: "Snowfield",
        land: (0.30, 1.0),
        relief: ANY,
        rock: ANY,
        heat: (0.0, 0.20),
        wet: ANY,
        weird: ANY,
        depth: (0.0, 0.05),
        daylight: true,
        priority: 2,
        members: vec![
            SubBiome {
                name: "Hoarfrost Barrens",
                lithology: Lithology::Frozen,
                stone: material::PERMAFROST,
                streak: material::BLUE_ICE,
                sediment: material::SNOW,
                skin: Some(Skin {
                    cover: material::SNOW,
                    cover_depth: 20,
                    soil: material::PERMAFROST,
                    soil_depth: 32,
                    subsoil: material::BLUE_ICE,
                    subsoil_depth: 50,
                    tree: Tree::DwarfWillow,
                    tree_spacing: 260,
                    ground_cover: Some(material::LICHEN),
                    cover_chance: 0.14,
                    ..turf_skin()
                }),
                ..daylight()
            },
            SubBiome {
                name: "Glacier Snout",
                variant: (0.56, 1.0),
                priority: 1,
                lithology: Lithology::Frozen,
                stone: material::BLUE_ICE,
                streak: material::PERMAFROST,
                sediment: material::SNOW,
                solidity: 0.34,
                skin: Some(Skin {
                    cover: material::SNOW,
                    cover_depth: 12,
                    soil: material::BLUE_ICE,
                    soil_depth: 40,
                    subsoil: material::BLUE_ICE,
                    subsoil_depth: 80,
                    ground_cover: None,
                    cover_chance: 0.0,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
        ],
    }
}

fn dunesea() -> Biome {
    Biome {
        name: "Dunesea",
        land: (0.34, 1.0),
        relief: ANY,
        rock: ANY,
        heat: (0.62, 1.0),
        wet: (0.0, 0.38),
        weird: (0.0, 0.70),
        depth: (0.0, 0.05),
        daylight: true,
        priority: 2,
        members: vec![
            SubBiome {
                name: "Dune Sea",
                lithology: Lithology::Clastic,
                stone: material::SANDSTONE,
                streak: material::SAND,
                sediment: material::SAND,
                skin: Some(Skin {
                    cover: material::SAND,
                    cover_depth: 3,
                    soil: material::SAND,
                    soil_depth: 44,
                    subsoil: material::SANDSTONE,
                    subsoil_depth: 60,
                    ground_cover: None,
                    cover_chance: 0.0,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
            SubBiome {
                name: "Salt Pan",
                wet: (0.24, 0.44),
                priority: 1,
                lithology: Lithology::Clastic,
                stone: material::SANDSTONE,
                streak: material::GYPSUM,
                sediment: material::SALT,
                prize: material::SALT,
                skin: Some(Skin {
                    cover: material::SALT,
                    cover_depth: 6,
                    soil: material::CLAY,
                    soil_depth: 34,
                    subsoil: material::SANDSTONE,
                    subsoil_depth: 40,
                    ground_cover: None,
                    cover_chance: 0.0,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
        ],
    }
}

fn coast() -> Biome {
    Biome {
        name: "Sunken Coast",
        land: (0.0, 0.30),
        relief: ANY,
        rock: ANY,
        heat: ANY,
        wet: ANY,
        weird: (0.0, 0.62),
        depth: (0.0, 0.05),
        daylight: true,
        priority: 3,
        members: vec![
            SubBiome {
                name: "Shoal Shelf",
                lithology: Lithology::Clastic,
                stone: material::SANDSTONE,
                streak: material::CLAY,
                sediment: material::SAND,
                skin: Some(Skin {
                    cover: material::SAND,
                    cover_depth: 4,
                    soil: material::SAND,
                    soil_depth: 24,
                    subsoil: material::CLAY,
                    subsoil_depth: 50,
                    ground_cover: Some(material::KELP),
                    cover_chance: 0.5,
                    submerged: true,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
            SubBiome {
                name: "Tidewrack Flats",
                variant: (0.54, 1.0),
                priority: 1,
                lithology: Lithology::Soil,
                stone: material::CLAY,
                streak: material::MUD,
                sediment: material::MUD,
                skin: Some(Skin {
                    cover: material::MUD,
                    cover_depth: 5,
                    soil: material::MUD,
                    soil_depth: 26,
                    subsoil: material::CLAY,
                    subsoil_depth: 44,
                    ground_cover: Some(material::REED),
                    cover_chance: 0.34,
                    submerged: true,
                    wade: 10,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
        ],
    }
}

fn saltmarsh() -> Biome {
    Biome {
        name: "Saltmarsh",
        land: (0.24, 0.46),
        relief: (0.40, 1.0),
        rock: ANY,
        heat: (0.48, 1.0),
        wet: (0.56, 1.0),
        weird: (0.0, 0.62),
        depth: (0.0, 0.05),
        daylight: true,
        priority: 3,
        members: vec![
            SubBiome {
                name: "Mangrove Sump",
                lithology: Lithology::Soil,
                stone: material::CLAY,
                streak: material::PEAT,
                sediment: material::MUD,
                skin: Some(Skin {
                    cover: material::MUD,
                    cover_depth: 3,
                    soil: material::PEAT,
                    soil_depth: 26,
                    subsoil: material::CLAY,
                    subsoil_depth: 44,
                    tree: Tree::Mangrove,
                    tree_spacing: 60,
                    ground_cover: Some(material::REED),
                    cover_chance: 0.45,
                    wade: 16,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
            SubBiome {
                name: "Reedbeds",
                variant: (0.52, 1.0),
                priority: 1,
                lithology: Lithology::Soil,
                stone: material::PEAT,
                streak: material::CLAY,
                sediment: material::MUD,
                skin: Some(Skin {
                    cover: material::PEAT,
                    cover_depth: 6,
                    soil: material::MUD,
                    soil_depth: 30,
                    subsoil: material::CLAY,
                    subsoil_depth: 40,
                    ground_cover: Some(material::REED),
                    cover_chance: 0.62,
                    wade: 12,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
        ],
    }
}

fn ashland() -> Biome {
    Biome {
        name: "Ashland",
        land: (0.32, 1.0),
        relief: ANY,
        rock: (0.54, 1.0),
        heat: (0.44, 1.0),
        wet: (0.0, 0.60),
        weird: (0.12, 1.0),
        depth: (0.0, 0.05),
        daylight: true,
        priority: 3,
        members: vec![
            SubBiome {
                name: "Basalt Colonnade",
                lithology: Lithology::Igneous,
                stone: material::BASALT,
                streak: material::OBSIDIAN,
                sediment: material::ASH,
                skin: Some(Skin {
                    cover: material::BASALT,
                    cover_depth: 2,
                    soil: material::ASH,
                    soil_depth: 8,
                    subsoil: material::BASALT,
                    subsoil_depth: 60,
                    ground_cover: None,
                    cover_chance: 0.0,
                    boulder_spacing: 300,
                    boulder: material::BASALT,
                    ..turf_skin()
                }),
                ..daylight()
            },
            SubBiome {
                name: "Ashen Barrens",
                variant: (0.48, 1.0),
                priority: 1,
                lithology: Lithology::Igneous,
                stone: material::BASALT,
                streak: material::CHARCOAL,
                sediment: material::ASH,
                skin: Some(Skin {
                    cover: material::ASH,
                    cover_depth: 14,
                    soil: material::CHARCOAL,
                    soil_depth: 8,
                    subsoil: material::BASALT,
                    subsoil_depth: 44,
                    tree: Tree::Snag,
                    tree_spacing: 110,
                    ground_cover: None,
                    cover_chance: 0.0,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
        ],
    }
}

fn sporehall() -> Biome {
    Biome {
        name: "Sporehall",
        land: (0.32, 1.0),
        relief: ANY,
        rock: ANY,
        heat: (0.30, 0.78),
        wet: (0.52, 1.0),
        weird: (0.28, 1.0),
        depth: (0.0, 0.05),
        daylight: true,
        priority: 4,
        members: vec![
            SubBiome {
                name: "Sporehall Clearing",
                lithology: Lithology::Fungal,
                stone: material::MYCOSTONE,
                streak: material::MYCELIUM,
                sediment: material::MYCELIUM,
                prize: material::AMBER,
                skin: Some(Skin {
                    cover: material::MYCELIUM,
                    cover_depth: 4,
                    soil: material::PEAT,
                    soil_depth: 22,
                    subsoil: material::MYCOSTONE,
                    subsoil_depth: 44,
                    tree: Tree::Sporecap,
                    tree_spacing: 54,
                    ground_cover: Some(material::GLOWCAP),
                    cover_chance: 0.3,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
            SubBiome {
                name: "Slime Flats",
                variant: (0.50, 1.0),
                priority: 1,
                lithology: Lithology::Fungal,
                stone: material::MYCOSTONE,
                streak: material::PEAT,
                sediment: material::MYCELIUM,
                fluid: Some(material::SLIME),
                fluid_level: len(-6),
                prize: material::AMBER,
                skin: Some(Skin {
                    cover: material::MYCELIUM,
                    cover_depth: 8,
                    soil: material::PEAT,
                    soil_depth: 26,
                    subsoil: material::MYCOSTONE,
                    subsoil_depth: 40,
                    tree: Tree::Sporecap,
                    tree_spacing: 140,
                    ground_cover: Some(material::MYCELIUM),
                    cover_chance: 0.55,
                    boulder_spacing: 0,
                    ..turf_skin()
                }),
                ..daylight()
            },
        ],
    }
}

fn mines() -> Biome {
    Biome {
        name: "The Mines",
        land: ANY,
        relief: ANY,
        rock: (0.38, 0.64),
        heat: ANY,
        wet: ANY,
        weird: (0.0, 0.72),
        depth: (0.05, 0.82),
        daylight: false,
        priority: 1,
        members: vec![
            SubBiome {
                name: "Granite Workings",
                lithology: Lithology::Crystalline,
                stone: material::GRANITE,
                streak: material::QUARTZ,
                sediment: material::GRAVEL,
                solidity: 0.34,
                worms: 0.95,
                chambers: 0.8,
                prize: material::TIN_ORE,
                ..buried()
            },
            SubBiome {
                name: "Oil Seeps",
                variant: (0.52, 1.0),
                priority: 1,
                lithology: Lithology::Crystalline,
                stone: material::GRANITE,
                streak: material::COAL,
                sediment: material::GRAVEL,
                fluid: Some(material::OIL),
                fluid_level: len(-900),
                gas: Some(material::FIREDAMP),
                gas_chance: 0.2,
                prize: material::NATIVE_COPPER,
                ..buried()
            },
            SubBiome {
                name: "Goldveins",
                depth: (0.40, 0.82),
                weird: (0.22, 1.0),
                priority: 2,
                lithology: Lithology::Crystalline,
                stone: material::GRANITE,
                streak: material::GOLD,
                sediment: material::GRAVEL,
                fluid: Some(material::LAVA),
                fluid_level: len(-6400),
                prize: material::GOLD,
                ..buried()
            },
        ],
    }
}

fn coalpits() -> Biome {
    Biome {
        name: "Coal Pits",
        land: ANY,
        relief: ANY,
        rock: (0.16, 0.42),
        heat: ANY,
        wet: (0.0, 0.66),
        weird: (0.0, 0.72),
        depth: (0.05, 0.78),
        daylight: false,
        priority: 2,
        members: vec![
            SubBiome {
                name: "Coal Measures",
                lithology: Lithology::Clastic,
                stone: material::SHALE,
                streak: material::COAL,
                sediment: material::COAL,
                gas: Some(material::FIREDAMP),
                gas_chance: 0.30,
                prize: material::COAL,
                ..buried()
            },
            SubBiome {
                name: "Firedamp Seams",
                variant: (0.56, 1.0),
                priority: 1,
                lithology: Lithology::Clastic,
                stone: material::SHALE,
                streak: material::COAL,
                sediment: material::COAL,
                fluid: Some(material::OIL),
                fluid_level: len(-1600),
                gas: Some(material::FIREDAMP),
                gas_chance: 0.52,
                solidity: 0.22,
                worms: 1.2,
                prize: material::SULFUR,
                ..buried()
            },
            SubBiome {
                name: "Tar Sink",
                depth: (0.24, 0.78),
                wet: (0.44, 1.0),
                priority: 2,
                lithology: Lithology::Clastic,
                stone: material::SHALE,
                streak: material::COAL,
                sediment: material::TAR,
                fluid: Some(material::TAR),
                fluid_level: len(-2400),
                gas: Some(material::FIREDAMP),
                gas_chance: 0.36,
                prize: material::AMBER,
                ..buried()
            },
        ],
    }
}

fn drowned() -> Biome {
    Biome {
        name: "Drowned Halls",
        land: ANY,
        relief: ANY,
        rock: (0.0, 0.26),
        heat: ANY,
        wet: ANY,
        weird: (0.0, 0.72),
        depth: (0.05, 0.72),
        daylight: false,
        priority: 1,
        members: vec![
            SubBiome {
                name: "Karst Halls",
                lithology: Lithology::Carbonate,
                stone: material::LIMESTONE,
                streak: material::GYPSUM,
                sediment: material::GRAVEL,
                solidity: 0.20,
                worms: 1.2,
                chambers: 1.5,
                galleries: 1.1,
                prize: material::FLINT,
                ..buried()
            },
            SubBiome {
                name: "Flooded Galleries",
                wet: (0.48, 1.0),
                priority: 1,
                lithology: Lithology::Carbonate,
                stone: material::LIMESTONE,
                streak: material::CLAY,
                sediment: material::MUD,
                fluid: Some(material::WATER),
                fluid_level: len(-1200),
                solidity: 0.17,
                chambers: 1.4,
                prize: material::NATIVE_COPPER,
                ..buried()
            },
            SubBiome {
                name: "Gypsum Vaults",
                variant: (0.62, 1.0),
                priority: 2,
                lithology: Lithology::Carbonate,
                stone: material::GYPSUM,
                streak: material::LIMESTONE,
                sediment: material::GRAVEL,
                solidity: 0.24,
                chambers: 1.6,
                prize: material::GYPSUM,
                ..buried()
            },
        ],
    }
}

fn sandtombs() -> Biome {
    Biome {
        name: "Sandtombs",
        land: ANY,
        relief: ANY,
        rock: ANY,
        heat: (0.66, 1.0),
        wet: (0.0, 0.36),
        weird: (0.0, 0.80),
        depth: (0.05, 0.52),
        daylight: false,
        priority: 2,
        members: vec![
            SubBiome {
                name: "Sand Warrens",
                lithology: Lithology::Clastic,
                stone: material::SANDSTONE,
                streak: material::SAND,
                sediment: material::SAND,
                solidity: 0.26,
                worms: 1.15,
                prize: material::SALTPETER,
                ..buried()
            },
            SubBiome {
                name: "Bone Ossuary",
                variant: (0.54, 1.0),
                priority: 1,
                lithology: Lithology::Carbonate,
                stone: material::BONE,
                streak: material::SANDSTONE,
                sediment: material::SAND,
                gas: Some(material::CHOKEDAMP),
                gas_chance: 0.28,
                prize: material::AMBER,
                ..buried()
            },
        ],
    }
}

fn fungal() -> Biome {
    Biome {
        name: "Fungal Caverns",
        land: ANY,
        relief: ANY,
        rock: ANY,
        heat: (0.26, 0.76),
        wet: (0.62, 1.0),
        weird: (0.16, 1.0),
        depth: (0.05, 0.62),
        daylight: false,
        priority: 3,
        members: vec![
            SubBiome {
                name: "Mycostone Bloom",
                lithology: Lithology::Fungal,
                stone: material::MYCOSTONE,
                streak: material::MYCELIUM,
                sediment: material::MYCELIUM,
                gas: Some(material::SPOREGAS),
                gas_chance: 0.30,
                solidity: 0.19,
                chambers: 1.5,
                prize: material::AMBER,
                ..buried()
            },
            SubBiome {
                name: "Slime Warrens",
                variant: (0.50, 1.0),
                priority: 1,
                lithology: Lithology::Fungal,
                stone: material::MYCOSTONE,
                streak: material::MYCELIUM,
                sediment: material::MYCELIUM,
                fluid: Some(material::SLIME),
                fluid_level: len(-1800),
                gas: Some(material::SPOREGAS),
                gas_chance: 0.42,
                solidity: 0.16,
                worms: 1.3,
                chambers: 1.7,
                prize: material::LUMEN,
                ..buried()
            },
        ],
    }
}

fn snowydepths() -> Biome {
    Biome {
        name: "Snowy Depths",
        land: ANY,
        relief: ANY,
        rock: ANY,
        heat: (0.0, 0.20),
        wet: ANY,
        weird: ANY,
        depth: (0.05, 0.58),
        daylight: false,
        priority: 2,
        members: vec![
            SubBiome {
                name: "Frostpan",
                lithology: Lithology::Frozen,
                stone: material::PERMAFROST,
                streak: material::BLUE_ICE,
                sediment: material::SNOW,
                solidity: 0.38,
                worms: 0.85,
                prize: material::QUARTZ,
                ..buried()
            },
            SubBiome {
                name: "Iceglass Hollows",
                variant: (0.52, 1.0),
                priority: 1,
                lithology: Lithology::Frozen,
                stone: material::BLUE_ICE,
                streak: material::PERMAFROST,
                sediment: material::SNOW,
                fluid: Some(material::WATER),
                fluid_level: len(-1100),
                solidity: 0.20,
                chambers: 1.5,
                prize: material::LUMEN,
                ..buried()
            },
        ],
    }
}

fn sludgeworks() -> Biome {
    Biome {
        name: "The Sludgeworks",
        land: ANY,
        relief: ANY,
        rock: ANY,
        heat: (0.28, 0.92),
        wet: (0.34, 1.0),
        weird: (0.24, 1.0),
        depth: (0.18, 0.86),
        daylight: false,
        priority: 4,
        members: vec![
            SubBiome {
                name: "Sludge Pools",
                lithology: Lithology::Clastic,
                stone: material::SHALE,
                streak: material::SULFUR,
                sediment: material::GRAVEL,
                fluid: Some(material::TOXIC_SLUDGE),
                fluid_level: len(-2200),
                gas: Some(material::TOXIC_GAS),
                gas_chance: 0.34,
                solidity: 0.20,
                chambers: 1.4,
                prize: material::SULFUR,
                ..buried()
            },
            SubBiome {
                name: "Fuming Terraces",
                variant: (0.54, 1.0),
                priority: 1,
                lithology: Lithology::Clastic,
                stone: material::SHALE,
                streak: material::SULFUR,
                sediment: material::SULFUR,
                fluid: Some(material::LAVA),
                fluid_level: len(-5200),
                gas: Some(material::TOXIC_GAS),
                gas_chance: 0.56,
                solidity: 0.24,
                galleries: 1.3,
                prize: material::SALTPETER,
                ..buried()
            },
        ],
    }
}

fn acidcaves() -> Biome {
    Biome {
        name: "Acid Caves",
        land: ANY,
        relief: ANY,
        rock: (0.0, 0.42),
        heat: (0.50, 1.0),
        wet: (0.36, 1.0),
        weird: ANY,
        depth: (0.22, 0.90),
        daylight: false,
        priority: 4,
        members: vec![
            SubBiome {
                name: "Acid Sumps",
                lithology: Lithology::Carbonate,
                stone: material::LIMESTONE,
                streak: material::SULFUR,
                sediment: material::GRAVEL,
                fluid: Some(material::ACID),
                fluid_level: len(-3000),
                gas: Some(material::TOXIC_GAS),
                gas_chance: 0.30,
                solidity: 0.18,
                chambers: 1.5,
                prize: material::GOLD,
                ..buried()
            },
            SubBiome {
                name: "Etched Halls",
                variant: (0.56, 1.0),
                priority: 1,
                lithology: Lithology::Carbonate,
                stone: material::LIMESTONE,
                streak: material::QUARTZ,
                sediment: material::GRAVEL,
                gas: Some(material::TOXIC_GAS),
                gas_chance: 0.24,
                solidity: 0.15,
                worms: 1.3,
                chambers: 1.8,
                prize: material::LUMEN,
                ..buried()
            },
        ],
    }
}

fn lavatubes() -> Biome {
    Biome {
        name: "Lava Tubes",
        land: ANY,
        relief: ANY,
        rock: (0.62, 1.0),
        heat: (0.50, 1.0),
        wet: ANY,
        weird: ANY,
        depth: (0.24, 1.0),
        daylight: false,
        priority: 2,
        members: vec![
            SubBiome {
                name: "Basalt Galleries",
                lithology: Lithology::Igneous,
                stone: material::BASALT,
                streak: material::OBSIDIAN,
                sediment: material::ASH,
                fluid: Some(material::LAVA),
                fluid_level: len(-4200),
                solidity: 0.30,
                galleries: 1.4,
                prize: material::IRON_ORE,
                ..buried()
            },
            SubBiome {
                name: "Obsidian Shelves",
                variant: (0.0, 0.42),
                depth: (0.42, 1.0),
                priority: 1,
                lithology: Lithology::Igneous,
                stone: material::OBSIDIAN,
                streak: material::BASALT,
                sediment: material::ASH,
                solidity: 0.38,
                prize: material::GOLD,
                ..buried()
            },
            SubBiome {
                name: "Ember Sea",
                depth: (0.50, 1.0),
                heat: (0.70, 1.0),
                priority: 2,
                lithology: Lithology::Igneous,
                stone: material::BASALT,
                streak: material::OBSIDIAN,
                sediment: material::ASH,
                fluid: Some(material::LAVA),
                fluid_level: len(-2600),
                solidity: 0.22,
                chambers: 1.4,
                prize: material::GOLD,
                ..buried()
            },
        ],
    }
}

fn crystal() -> Biome {
    Biome {
        name: "Crystal Hollows",
        land: ANY,
        relief: ANY,
        rock: ANY,
        heat: ANY,
        wet: (0.0, 0.68),
        weird: (0.28, 1.0),
        depth: (0.34, 1.0),
        daylight: false,
        priority: 3,
        members: vec![
            SubBiome {
                name: "Quartz Hollows",
                lithology: Lithology::Crystalline,
                stone: material::QUARTZ,
                streak: material::GRANITE,
                sediment: material::GRAVEL,
                solidity: 0.24,
                chambers: 1.6,
                prize: material::QUARTZ,
                ..buried()
            },
            SubBiome {
                name: "Lumen Hollows",
                variant: (0.50, 1.0),
                priority: 1,
                lithology: Lithology::Crystalline,
                stone: material::QUARTZ,
                streak: material::QUARTZ,
                sediment: material::GRAVEL,
                gas: Some(material::VOIDMIST),
                gas_chance: 0.26,
                solidity: 0.20,
                chambers: 1.8,
                prize: material::LUMEN,
                ..buried()
            },
        ],
    }
}

fn meatgrounds() -> Biome {
    Biome {
        name: "The Meatgrounds",
        land: ANY,
        relief: ANY,
        rock: ANY,
        heat: ANY,
        wet: ANY,
        weird: ANY,
        depth: (0.74, 1.0),
        daylight: false,
        priority: 2,
        members: vec![
            SubBiome {
                name: "Titanflesh Weft",
                lithology: Lithology::Flesh,
                stone: material::TITANFLESH,
                streak: material::BONE,
                sediment: material::BONE,
                gas: Some(material::VOIDMIST),
                gas_chance: 0.30,
                solidity: 0.18,
                chambers: 1.6,
                prize: material::LUMEN,
                ..buried()
            },
            SubBiome {
                name: "Blood Marsh",
                wet: (0.54, 1.0),
                priority: 1,
                lithology: Lithology::Flesh,
                stone: material::TITANFLESH,
                streak: material::BONE,
                sediment: material::BONE,
                fluid: Some(material::BLOOD),
                fluid_level: len(-8600),
                gas: Some(material::CHOKEDAMP),
                gas_chance: 0.26,
                solidity: 0.14,
                prize: material::GOLD,
                ..buried()
            },
            SubBiome {
                name: "Hollow Choir",
                variant: (0.58, 1.0),
                priority: 2,
                lithology: Lithology::Flesh,
                stone: material::BONE,
                streak: material::TITANFLESH,
                sediment: material::BONE,
                fluid: Some(material::ICHOR),
                fluid_level: len(-14000),
                gas: Some(material::VOIDMIST),
                gas_chance: 0.36,
                solidity: 0.12,
                chambers: 1.9,
                galleries: 1.4,
                prize: material::LUMEN,
                ..buried()
            },
        ],
    }
}

pub fn biomes() -> Vec<Biome> {
    vec![
        meadow(),
        pinewood(),
        snowfield(),
        dunesea(),
        coast(),
        saltmarsh(),
        ashland(),
        sporehall(),
        mines(),
        coalpits(),
        drowned(),
        sandtombs(),
        fungal(),
        snowydepths(),
        sludgeworks(),
        acidcaves(),
        lavatubes(),
        crystal(),
        meatgrounds(),
    ]
}

pub fn pick_biome(biomes: &[Biome], params: &Params, daylight: bool) -> usize {
    let mut best = 0;
    let mut best_miss = f32::MAX;
    let mut best_priority = 0;
    for (index, biome) in biomes.iter().enumerate() {
        if biome.daylight != daylight {
            continue;
        }
        let miss = 16.0 * squared_gap(params.depth, biome.depth)
            + 2.4 * squared_gap(params.rock, biome.rock)
            + squared_gap(params.land, biome.land)
            + squared_gap(params.relief, biome.relief)
            + squared_gap(params.heat, biome.heat)
            + squared_gap(params.wet, biome.wet)
            + squared_gap(params.weird, biome.weird);
        if miss < best_miss || (miss == best_miss && biome.priority > best_priority) {
            best_miss = miss;
            best_priority = biome.priority;
            best = index;
        }
    }
    best
}

pub fn pick_sub(members: &[SubBiome], params: &Params) -> usize {
    let mut best = 0;
    let mut best_miss = f32::MAX;
    let mut best_priority = 0;
    for (index, member) in members.iter().enumerate() {
        let miss = 3.0 * squared_gap(params.depth, member.depth)
            + squared_gap(params.heat, member.heat)
            + squared_gap(params.wet, member.wet)
            + squared_gap(params.weird, member.weird)
            + squared_gap(params.variant, member.variant);
        if miss < best_miss || (miss == best_miss && member.priority > best_priority) {
            best_miss = miss;
            best_priority = member.priority;
            best = index;
        }
    }
    best
}

fn squared_gap(value: f32, range: (f32, f32)) -> f32 {
    let gap = if value < range.0 {
        range.0 - value
    } else if value > range.1 {
        value - range.1
    } else {
        return 0.0;
    };
    gap * gap
}
