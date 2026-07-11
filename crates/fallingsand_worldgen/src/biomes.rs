use fallingsand_core::MaterialId;
use fallingsand_core::content::material;

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

pub(crate) struct TreeDef {
    pub density: f32,
    pub spacing: i32,
    pub trunk_height: (i32, i32),
    pub wood: MaterialId,
    pub leaves: MaterialId,
    pub canopy: Canopy,
    pub snow_capped: bool,
}

pub(crate) struct Biome {
    #[allow(dead_code)]
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
    pub snow_cover: bool,
    pub vine_chance: f32,
}

pub struct Band {
    #[allow(dead_code)]
    pub name: &'static str,
    pub floor: Option<i32>,
    pub stone: MaterialId,
    pub cave_threshold: f32,
    pub cave_depth_bonus: f32,
    pub lava_pools: bool,
    pub lava_pool_threshold: f32,
    pub aquifers: bool,
}

pub(crate) struct OreDef {
    pub material: MaterialId,
    pub min_y: i32,
    pub max_y: i32,
    pub chance: f32,
    pub steps: (i32, i32),
    pub radius: (i32, i32),
}

pub(crate) struct WorldDef {
    pub sea_level: i32,
    pub biomes: Vec<Biome>,
    pub bands: Vec<Band>,
    pub ores: Vec<OreDef>,
}

pub const MAX_OVERHANG: i32 = 48;
pub const OVERHANG_AMPLITUDE: f32 = 26.0;
pub const OVERHANG_FADE: f32 = 0.35;
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
pub const SNOW_LINE: i32 = 140;
pub const SNOW_COVER_DEPTH: i32 = 2;
pub const DECOR_COLUMN_CHANCE: f32 = 0.30;
pub const DECOR_SCAN_FLOOR: i32 = -340;
pub const VINE_MAX_DEPTH: i32 = 80;
pub const MUSHROOM_ANCHOR_GRID: i32 = 96;
pub const MUSHROOM_CHANCE: f32 = 0.30;
pub const MUSHROOM_MIN_Y: i32 = -330;
pub const MUSHROOM_MAX_Y: i32 = -30;
pub const RUIN_ANCHOR_GRID: i32 = 448;
pub const RUIN_CHANCE: f32 = 0.30;
pub const MINESHAFT_ANCHOR_X: i32 = 416;
pub const MINESHAFT_ANCHOR_Y: i32 = 224;
pub const MINESHAFT_CHANCE: f32 = 0.22;
pub const MINESHAFT_MIN_Y: i32 = -640;
pub const MINESHAFT_MAX_Y: i32 = -100;
pub const ISLAND_ANCHOR_X: i32 = 448;
pub const ISLAND_ANCHOR_Y: i32 = 256;
pub const ISLAND_CHANCE: f32 = 0.22;
pub const ISLAND_MIN_Y: i32 = 220;
pub const ISLAND_MAX_Y: i32 = 1400;
pub const STRUCTURE_MARGIN: i32 = 200;

pub(crate) fn world_def() -> WorldDef {
    WorldDef {
        sea_level: -10,
        biomes: vec![
            Biome {
                name: "meadow",
                surface: material::GRASS,
                soil: material::DIRT,
                soil_depth: 14,
                underlayer: None,
                height_amplitude: 28.0,
                ruggedness: 0.35,
                tree: Some(TreeDef {
                    density: 0.06,
                    spacing: 11,
                    trunk_height: (14, 26),
                    wood: material::WOOD,
                    leaves: material::LEAVES,
                    canopy: Canopy::Round,
                    snow_capped: false,
                }),
                tuft_chance: 0.28,
                beach: Beach::Sand,
                pond_chance: 0.35,
                shallow_aquifer: None,
                snow_cover: false,
                vine_chance: 0.06,
            },
            Biome {
                name: "forest",
                surface: material::GRASS,
                soil: material::DIRT,
                soil_depth: 12,
                underlayer: None,
                height_amplitude: 46.0,
                ruggedness: 0.55,
                tree: Some(TreeDef {
                    density: 0.38,
                    spacing: 6,
                    trunk_height: (16, 34),
                    wood: material::WOOD,
                    leaves: material::LEAVES,
                    canopy: Canopy::Round,
                    snow_capped: false,
                }),
                tuft_chance: 0.18,
                beach: Beach::Sand,
                pond_chance: 0.25,
                shallow_aquifer: None,
                snow_cover: false,
                vine_chance: 0.14,
            },
            Biome {
                name: "desert",
                surface: material::SAND,
                soil: material::SAND,
                soil_depth: 16,
                underlayer: Some((material::SANDSTONE, 30)),
                height_amplitude: 20.0,
                ruggedness: 0.18,
                tree: None,
                tuft_chance: 0.0,
                beach: Beach::Sand,
                pond_chance: 0.0,
                shallow_aquifer: None,
                snow_cover: false,
                vine_chance: 0.0,
            },
            Biome {
                name: "rocklands",
                surface: material::STONE,
                soil: material::GRAVEL,
                soil_depth: 6,
                underlayer: None,
                height_amplitude: 95.0,
                ruggedness: 1.3,
                tree: None,
                tuft_chance: 0.0,
                beach: Beach::Sand,
                pond_chance: 0.0,
                shallow_aquifer: None,
                snow_cover: false,
                vine_chance: 0.03,
            },
            Biome {
                name: "snowlands",
                surface: material::SNOW,
                soil: material::DIRT,
                soil_depth: 10,
                underlayer: None,
                height_amplitude: 52.0,
                ruggedness: 0.6,
                tree: Some(TreeDef {
                    density: 0.14,
                    spacing: 8,
                    trunk_height: (12, 24),
                    wood: material::WOOD,
                    leaves: material::LEAVES,
                    canopy: Canopy::Conifer,
                    snow_capped: true,
                }),
                tuft_chance: 0.0,
                beach: Beach::Ice,
                pond_chance: 0.15,
                shallow_aquifer: None,
                snow_cover: true,
                vine_chance: 0.0,
            },
            Biome {
                name: "swamp",
                surface: material::GRASS,
                soil: material::MUD,
                soil_depth: 14,
                underlayer: Some((material::CLAY, 12)),
                height_amplitude: 7.0,
                ruggedness: 0.2,
                tree: Some(TreeDef {
                    density: 0.2,
                    spacing: 9,
                    trunk_height: (12, 22),
                    wood: material::WOOD,
                    leaves: material::LEAVES,
                    canopy: Canopy::Round,
                    snow_capped: false,
                }),
                tuft_chance: 0.32,
                beach: Beach::Sand,
                pond_chance: 0.8,
                shallow_aquifer: Some(material::OIL),
                snow_cover: false,
                vine_chance: 0.30,
            },
        ],
        bands: vec![
            Band {
                name: "crust",
                floor: Some(-350),
                stone: material::STONE,
                cave_threshold: 0.11,
                cave_depth_bonus: 0.05,
                lava_pools: false,
                lava_pool_threshold: 1.0,
                aquifers: true,
            },
            Band {
                name: "deep",
                floor: Some(-900),
                stone: material::DEEPSTONE,
                cave_threshold: 0.13,
                cave_depth_bonus: 0.04,
                lava_pools: true,
                lava_pool_threshold: 0.42,
                aquifers: false,
            },
            Band {
                name: "molten",
                floor: None,
                stone: material::BASALT,
                cave_threshold: 0.15,
                cave_depth_bonus: 0.0,
                lava_pools: true,
                lava_pool_threshold: 0.22,
                aquifers: false,
            },
        ],
        ores: vec![
            OreDef {
                material: material::COAL,
                min_y: -260,
                max_y: 40,
                chance: 0.16,
                steps: (5, 12),
                radius: (1, 2),
            },
            OreDef {
                material: material::IRON_ORE,
                min_y: -700,
                max_y: -120,
                chance: 0.13,
                steps: (5, 10),
                radius: (1, 2),
            },
            OreDef {
                material: material::GOLD_ORE,
                min_y: i32::MIN,
                max_y: -480,
                chance: 0.10,
                steps: (4, 8),
                radius: (1, 2),
            },
            OreDef {
                material: material::CRYSTAL,
                min_y: -900,
                max_y: -380,
                chance: 0.08,
                steps: (3, 6),
                radius: (2, 3),
            },
            OreDef {
                material: material::DIRT,
                min_y: -320,
                max_y: 30,
                chance: 0.22,
                steps: (6, 14),
                radius: (2, 3),
            },
            OreDef {
                material: material::GRAVEL,
                min_y: -620,
                max_y: 0,
                chance: 0.18,
                steps: (6, 12),
                radius: (2, 3),
            },
            OreDef {
                material: material::CLAY,
                min_y: -180,
                max_y: 15,
                chance: 0.14,
                steps: (5, 10),
                radius: (2, 3),
            },
            OreDef {
                material: material::SAND,
                min_y: -130,
                max_y: 25,
                chance: 0.12,
                steps: (5, 10),
                radius: (2, 3),
            },
            OreDef {
                material: material::BASALT,
                min_y: -880,
                max_y: -420,
                chance: 0.10,
                steps: (6, 12),
                radius: (2, 3),
            },
        ],
    }
}
