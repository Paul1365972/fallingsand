use crate::GenError;
use fallingsand_core::{MaterialId, MaterialRegistry};

pub struct Palette {
    pub stone: MaterialId,
    pub dirt: MaterialId,
    pub grass: MaterialId,
    pub sand: MaterialId,
    pub gravel: MaterialId,
    pub water: MaterialId,
    pub lava: MaterialId,
    pub oil: MaterialId,
    pub wood: MaterialId,
    pub leaves: MaterialId,
    pub moss: MaterialId,
    pub snow: MaterialId,
    pub ice: MaterialId,
    pub mud: MaterialId,
    pub clay: MaterialId,
    pub sandstone: MaterialId,
    pub deepstone: MaterialId,
    pub basalt: MaterialId,
    pub coal: MaterialId,
    pub iron_ore: MaterialId,
    pub gold_ore: MaterialId,
}

impl Palette {
    pub fn resolve(registry: &MaterialRegistry) -> Result<Self, GenError> {
        let id = |name: &str| {
            registry
                .id_of(name)
                .ok_or_else(|| GenError::UnknownMaterial(name.to_string()))
        };
        Ok(Self {
            stone: id("stone")?,
            dirt: id("dirt")?,
            grass: id("grass")?,
            sand: id("sand")?,
            gravel: id("gravel")?,
            water: id("water")?,
            lava: id("lava")?,
            oil: id("oil")?,
            wood: id("wood")?,
            leaves: id("leaves")?,
            moss: id("moss")?,
            snow: id("snow")?,
            ice: id("ice")?,
            mud: id("mud")?,
            clay: id("clay")?,
            sandstone: id("sandstone")?,
            deepstone: id("deepstone")?,
            basalt: id("basalt")?,
            coal: id("coal")?,
            iron_ore: id("iron_ore")?,
            gold_ore: id("gold_ore")?,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Beach {
    Sand,
    Ice,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Canopy {
    Round,
    Conifer,
}

pub struct TreeDef {
    pub density: f32,
    pub spacing: i32,
    pub trunk_height: (i32, i32),
    pub wood: MaterialId,
    pub leaves: MaterialId,
    pub canopy: Canopy,
}

pub struct Biome {
    pub name: &'static str,
    pub surface: MaterialId,
    pub soil: MaterialId,
    pub soil_depth: i32,
    pub underlayer: Option<(MaterialId, i32)>,
    pub height_amplitude: f32,
    pub ruggedness: f32,
    pub tree: Option<TreeDef>,
    pub tuft_chance: f32,
    pub beach: Beach,
    pub pond_chance: f32,
    pub shallow_aquifer: Option<MaterialId>,
}

pub struct Band {
    pub name: &'static str,
    pub floor: Option<i32>,
    pub stone: MaterialId,
    pub cave_threshold: f32,
    pub cave_depth_bonus: f32,
    pub lava_pools: bool,
    pub lava_pool_threshold: f32,
    pub aquifers: bool,
}

pub struct OreDef {
    pub material: MaterialId,
    pub min_y: i32,
    pub max_y: i32,
    pub chance: f32,
    pub steps: (i32, i32),
    pub radius: (i32, i32),
}

pub struct WorldDef {
    pub sea_level: i32,
    pub biomes: Vec<Biome>,
    pub bands: Vec<Band>,
    pub ores: Vec<OreDef>,
}

pub const MAX_OVERHANG: i32 = 48;
pub const OVERHANG_AMPLITUDE: f32 = 26.0;
pub const BEACH_RANGE: i32 = 3;
pub const BEACH_DEPTH: i32 = 6;
pub const SOIL_TRANSITION_JITTER: f32 = 2.0;
pub const AQUIFER_MIN_DEPTH: f32 = 24.0;
pub const AQUIFER_THRESHOLD: f32 = 0.55;
pub const SHALLOW_AQUIFER_FLOOR: i32 = -90;
pub const MOSS_CHANCE: f32 = 0.10;
pub const MOSS_MAX_DEPTH: i32 = 350;
pub const CAVE_SURFACE_GATE: f32 = 6.0;
pub const CAVERN_MIN_DEPTH: f32 = 60.0;
pub const CAVERN_RARITY_THRESHOLD: f32 = 0.20;
pub const CAVERN_CHEESE_THRESHOLD: f32 = 0.38;
pub const SHAFT_WIDTH: f32 = 0.02;
pub const ORE_ANCHOR_GRID: i32 = 32;
pub const ORE_MARGIN: i32 = 48;
pub const TREE_MARGIN: i32 = 48;
pub const POND_ANCHOR_GRID: i32 = 192;

pub fn world_def(palette: &Palette) -> WorldDef {
    WorldDef {
        sea_level: -10,
        biomes: vec![
            Biome {
                name: "meadow",
                surface: palette.grass,
                soil: palette.dirt,
                soil_depth: 14,
                underlayer: None,
                height_amplitude: 28.0,
                ruggedness: 0.35,
                tree: Some(TreeDef {
                    density: 0.06,
                    spacing: 11,
                    trunk_height: (14, 26),
                    wood: palette.wood,
                    leaves: palette.leaves,
                    canopy: Canopy::Round,
                }),
                tuft_chance: 0.28,
                beach: Beach::Sand,
                pond_chance: 0.35,
                shallow_aquifer: None,
            },
            Biome {
                name: "forest",
                surface: palette.grass,
                soil: palette.dirt,
                soil_depth: 12,
                underlayer: None,
                height_amplitude: 46.0,
                ruggedness: 0.55,
                tree: Some(TreeDef {
                    density: 0.38,
                    spacing: 6,
                    trunk_height: (16, 34),
                    wood: palette.wood,
                    leaves: palette.leaves,
                    canopy: Canopy::Round,
                }),
                tuft_chance: 0.18,
                beach: Beach::Sand,
                pond_chance: 0.25,
                shallow_aquifer: None,
            },
            Biome {
                name: "desert",
                surface: palette.sand,
                soil: palette.sand,
                soil_depth: 16,
                underlayer: Some((palette.sandstone, 30)),
                height_amplitude: 20.0,
                ruggedness: 0.18,
                tree: None,
                tuft_chance: 0.0,
                beach: Beach::Sand,
                pond_chance: 0.0,
                shallow_aquifer: None,
            },
            Biome {
                name: "rocklands",
                surface: palette.stone,
                soil: palette.gravel,
                soil_depth: 6,
                underlayer: None,
                height_amplitude: 95.0,
                ruggedness: 1.3,
                tree: None,
                tuft_chance: 0.0,
                beach: Beach::Sand,
                pond_chance: 0.0,
                shallow_aquifer: None,
            },
            Biome {
                name: "snowlands",
                surface: palette.snow,
                soil: palette.dirt,
                soil_depth: 10,
                underlayer: None,
                height_amplitude: 52.0,
                ruggedness: 0.6,
                tree: Some(TreeDef {
                    density: 0.14,
                    spacing: 8,
                    trunk_height: (12, 24),
                    wood: palette.wood,
                    leaves: palette.leaves,
                    canopy: Canopy::Conifer,
                }),
                tuft_chance: 0.0,
                beach: Beach::Ice,
                pond_chance: 0.15,
                shallow_aquifer: None,
            },
            Biome {
                name: "swamp",
                surface: palette.grass,
                soil: palette.mud,
                soil_depth: 14,
                underlayer: Some((palette.clay, 12)),
                height_amplitude: 7.0,
                ruggedness: 0.2,
                tree: Some(TreeDef {
                    density: 0.2,
                    spacing: 9,
                    trunk_height: (12, 22),
                    wood: palette.wood,
                    leaves: palette.leaves,
                    canopy: Canopy::Round,
                }),
                tuft_chance: 0.32,
                beach: Beach::Sand,
                pond_chance: 0.8,
                shallow_aquifer: Some(palette.oil),
            },
        ],
        bands: vec![
            Band {
                name: "crust",
                floor: Some(-350),
                stone: palette.stone,
                cave_threshold: 0.11,
                cave_depth_bonus: 0.05,
                lava_pools: false,
                lava_pool_threshold: 1.0,
                aquifers: true,
            },
            Band {
                name: "deep",
                floor: Some(-900),
                stone: palette.deepstone,
                cave_threshold: 0.13,
                cave_depth_bonus: 0.04,
                lava_pools: true,
                lava_pool_threshold: 0.42,
                aquifers: false,
            },
            Band {
                name: "molten",
                floor: None,
                stone: palette.basalt,
                cave_threshold: 0.15,
                cave_depth_bonus: 0.0,
                lava_pools: true,
                lava_pool_threshold: 0.22,
                aquifers: false,
            },
        ],
        ores: vec![
            OreDef {
                material: palette.coal,
                min_y: -260,
                max_y: 40,
                chance: 0.16,
                steps: (5, 12),
                radius: (1, 2),
            },
            OreDef {
                material: palette.iron_ore,
                min_y: -700,
                max_y: -120,
                chance: 0.13,
                steps: (5, 10),
                radius: (1, 2),
            },
            OreDef {
                material: palette.gold_ore,
                min_y: i32::MIN,
                max_y: -480,
                chance: 0.10,
                steps: (4, 8),
                radius: (1, 2),
            },
        ],
    }
}
