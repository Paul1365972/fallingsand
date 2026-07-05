use fastnoise_lite::{DomainWarpType, FastNoiseLite, FractalType, NoiseType};

use fallingsand_core::{
    Cell, CellOffset, ChunkOffset, DirtyRect, MaterialId, MaterialRegistry, REGION_SIZE_CELLS,
    REGION_SIZE_CHUNKS, Region, RegionPos,
};
use serde::Deserialize;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Deserialize)]
pub struct BiomeDef {
    pub name: String,
    pub surface_material: String,
    pub soil_material: String,
    pub soil_depth: i32,
    pub height_amplitude: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BandDef {
    pub name: String,
    pub floor: Option<i32>,
    pub stone_material: String,
    pub cave_threshold: f32,
    pub cave_depth_bonus: f32,
    #[serde(default)]
    pub lava_pools: bool,
    #[serde(default = "default_pool_threshold")]
    pub lava_pool_threshold: f32,
}

fn default_pool_threshold() -> f32 {
    0.35
}

#[derive(Debug, Clone, Deserialize)]
pub struct BiomeFile {
    pub sea_level: i32,
    pub bands: Vec<BandDef>,
    pub biomes: Vec<BiomeDef>,
}

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("failed to parse biomes: {0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("biome {biome:?} references unknown material {material:?}")]
    UnknownMaterial { biome: String, material: String },
    #[error("no biomes defined")]
    NoBiomes,
    #[error("invalid bands: {0}")]
    BadBands(String),
}

struct Biome {
    surface: MaterialId,
    soil: MaterialId,
    soil_depth: i32,
    height_amplitude: f32,
}

struct Band {
    floor: Option<i32>,
    stone: MaterialId,
    cave_threshold: f32,
    cave_depth_bonus: f32,
    lava_pools: bool,
    lava_pool_threshold: f32,
}

pub struct WorldGenerator {
    seed: u64,
    sea_level: i32,
    biomes: Vec<Biome>,
    bands: Vec<Band>,
    water: MaterialId,
    lava: MaterialId,
    height_noise: FastNoiseLite,
    biome_noise: FastNoiseLite,
    cave_noise: FastNoiseLite,
    cave_warp: FastNoiseLite,
    band_edge_noise: FastNoiseLite,
    lava_noise: FastNoiseLite,
}

fn sub_seed(seed: u64, purpose: &str) -> i32 {
    let mut hasher = rustc_hash::FxHasher::default();
    (seed, purpose).hash(&mut hasher);
    hasher.finish() as i32
}

impl WorldGenerator {
    pub fn new(
        seed: u64,
        registry: &MaterialRegistry,
        biomes_source: &str,
    ) -> Result<Self, GenError> {
        let file: BiomeFile = ron::from_str(biomes_source)?;
        if file.biomes.is_empty() {
            return Err(GenError::NoBiomes);
        }
        if file.bands.is_empty() {
            return Err(GenError::BadBands("no bands defined".to_string()));
        }
        for (index, pair) in file.bands.windows(2).enumerate() {
            match (pair[0].floor, pair[1].floor) {
                (None, _) => {
                    return Err(GenError::BadBands(format!(
                        "band {:?} has no floor but is not last",
                        file.bands[index].name
                    )));
                }
                (Some(upper), Some(lower)) if lower >= upper => {
                    return Err(GenError::BadBands(format!(
                        "band floors must strictly decrease, {:?} -> {:?}",
                        file.bands[index].name,
                        file.bands[index + 1].name
                    )));
                }
                _ => {}
            }
        }
        if file.bands.last().and_then(|band| band.floor).is_some() {
            return Err(GenError::BadBands(
                "last band must have floor: None".to_string(),
            ));
        }
        let resolve = |biome: &str, material: &str| {
            registry
                .id_of(material)
                .ok_or_else(|| GenError::UnknownMaterial {
                    biome: biome.to_string(),
                    material: material.to_string(),
                })
        };
        let biomes = file
            .biomes
            .iter()
            .map(|def| {
                Ok(Biome {
                    surface: resolve(&def.name, &def.surface_material)?,
                    soil: resolve(&def.name, &def.soil_material)?,
                    soil_depth: def.soil_depth,
                    height_amplitude: def.height_amplitude,
                })
            })
            .collect::<Result<Vec<_>, GenError>>()?;
        let water = resolve("<builtin>", "water")?;
        let lava = resolve("<builtin>", "lava")?;
        let bands = file
            .bands
            .iter()
            .map(|def| {
                Ok(Band {
                    floor: def.floor,
                    stone: resolve(&def.name, &def.stone_material)?,
                    cave_threshold: def.cave_threshold,
                    cave_depth_bonus: def.cave_depth_bonus,
                    lava_pools: def.lava_pools,
                    lava_pool_threshold: def.lava_pool_threshold,
                })
            })
            .collect::<Result<Vec<_>, GenError>>()?;

        let mut height_noise = FastNoiseLite::with_seed(sub_seed(seed, "height"));
        height_noise.set_noise_type(Some(NoiseType::OpenSimplex2));
        height_noise.set_fractal_type(Some(FractalType::FBm));
        height_noise.set_fractal_octaves(Some(4));
        height_noise.set_frequency(Some(0.003));

        let mut biome_noise = FastNoiseLite::with_seed(sub_seed(seed, "biome"));
        biome_noise.set_noise_type(Some(NoiseType::OpenSimplex2));
        biome_noise.set_frequency(Some(0.0006));

        let mut cave_noise = FastNoiseLite::with_seed(sub_seed(seed, "cave"));
        cave_noise.set_noise_type(Some(NoiseType::OpenSimplex2S));
        cave_noise.set_fractal_type(Some(FractalType::FBm));
        cave_noise.set_fractal_octaves(Some(3));
        cave_noise.set_frequency(Some(0.012));

        let mut cave_warp = FastNoiseLite::with_seed(sub_seed(seed, "cave_warp"));
        cave_warp.set_domain_warp_type(Some(DomainWarpType::OpenSimplex2));
        cave_warp.set_domain_warp_amp(Some(40.0));
        cave_warp.set_frequency(Some(0.008));
        cave_warp.set_fractal_type(Some(FractalType::DomainWarpProgressive));
        cave_warp.set_fractal_octaves(Some(3));

        let mut band_edge_noise = FastNoiseLite::with_seed(sub_seed(seed, "band_edge"));
        band_edge_noise.set_noise_type(Some(NoiseType::OpenSimplex2));
        band_edge_noise.set_frequency(Some(0.004));

        let mut lava_noise = FastNoiseLite::with_seed(sub_seed(seed, "lava"));
        lava_noise.set_noise_type(Some(NoiseType::OpenSimplex2S));
        lava_noise.set_fractal_type(Some(FractalType::FBm));
        lava_noise.set_fractal_octaves(Some(2));
        lava_noise.set_frequency(Some(0.015));

        Ok(Self {
            seed,
            sea_level: file.sea_level,
            biomes,
            bands,
            water,
            lava,
            height_noise,
            biome_noise,
            cave_noise,
            cave_warp,
            band_edge_noise,
            lava_noise,
        })
    }

    pub const fn seed(&self) -> u64 {
        self.seed
    }

    fn biome_mix(&self, x: i32) -> (usize, usize, f32) {
        let count = self.biomes.len() as f32;
        let t = (self.biome_noise.get_noise_2d(x as f32, 0.0) + 1.0) * 0.5 * count;
        let scaled = t.clamp(0.0, count - 1e-3);
        let index = scaled as usize;
        let frac = scaled - index as f32;
        let blend_band = 0.15f32;
        if frac > 1.0 - blend_band && index + 1 < self.biomes.len() {
            let mix = (frac - (1.0 - blend_band)) / (2.0 * blend_band);
            (index, index + 1, mix.clamp(0.0, 0.5))
        } else if frac < blend_band && index > 0 {
            let mix = 0.5 + frac / (2.0 * blend_band);
            (index - 1, index, mix.clamp(0.5, 1.0))
        } else {
            (index, index, 0.0)
        }
    }

    pub fn surface_height(&self, x: i32) -> i32 {
        let (a, b, mix) = self.biome_mix(x);
        let amplitude =
            self.biomes[a].height_amplitude * (1.0 - mix) + self.biomes[b].height_amplitude * mix;
        let base = self.height_noise.get_noise_2d(x as f32, 0.0);
        let detail = self
            .height_noise
            .get_noise_2d(x as f32 * 7.0 + 5000.0, 100.0);
        (base * amplitude + detail * 3.0) as i32
    }

    pub fn biome_at(&self, x: i32) -> usize {
        let (a, b, mix) = self.biome_mix(x);
        if mix < 0.5 { a } else { b }
    }

    fn band_at(&self, x: i32, y: i32) -> &Band {
        for (index, band) in self.bands.iter().enumerate() {
            let Some(floor) = band.floor else {
                return band;
            };
            let jitter = self
                .band_edge_noise
                .get_noise_2d(x as f32, index as f32 * 1000.0)
                * 24.0;
            if y as f32 >= floor as f32 + jitter {
                return band;
            }
        }
        self.bands.last().expect("bands validated non-empty")
    }

    fn is_cave(&self, x: i32, y: i32, surface: i32, band: &Band) -> bool {
        if y > surface - 6 {
            return false;
        }
        let depth_factor = ((surface - y) as f32 / 500.0).min(1.0);
        let threshold = band.cave_threshold + band.cave_depth_bonus * depth_factor;
        let (wx, wy) = self.cave_warp.domain_warp_2d(x as f32, y as f32);
        self.cave_noise.get_noise_2d(wx, wy).abs() < threshold
    }

    pub fn generate_region(&self, pos: RegionPos) -> Region {
        let mut region = Region::new();
        let base = pos.base_chunk().base_cell();

        let mut surfaces = [0i32; REGION_SIZE_CELLS];
        let mut biomes = [0usize; REGION_SIZE_CELLS];
        for (column, surface) in surfaces.iter_mut().enumerate() {
            let x = base.x + column as i32;
            *surface = self.surface_height(x);
            biomes[column] = self.biome_at(x);
        }

        for chunk_index in 0..REGION_SIZE_CHUNKS * REGION_SIZE_CHUNKS {
            let offset = ChunkOffset::from_index(chunk_index);
            let chunk = region.chunk_mut(offset);
            let chunk_base_x = base.x + offset.x as i32 * 64;
            let chunk_base_y = base.y + offset.y as i32 * 64;
            for local_y in 0..64u8 {
                let y = chunk_base_y + local_y as i32;
                for local_x in 0..64u8 {
                    let column = offset.x as usize * 64 + local_x as usize;
                    let x = chunk_base_x + local_x as i32;
                    let surface = surfaces[column];
                    let cell = self.cell_for(x, y, surface, biomes[column]);
                    if cell.material != MaterialId::AIR {
                        chunk.cells_mut()[CellOffset::new(local_x, local_y).index()] = cell;
                    }
                }
            }
            chunk.bounds = DirtyRect::FULL;
        }
        region
    }

    fn cell_for(&self, x: i32, y: i32, surface: i32, biome_index: usize) -> Cell {
        let biome = &self.biomes[biome_index];
        if y > surface {
            if y <= self.sea_level {
                return shaded(self.water, x, y);
            }
            return Cell::AIR;
        }
        let band = self.band_at(x, y);
        if self.is_cave(x, y, surface, band) {
            if band.lava_pools
                && self.lava_noise.get_noise_2d(x as f32, y as f32) > band.lava_pool_threshold
            {
                return shaded(self.lava, x, y);
            }
            return Cell::AIR;
        }
        let depth = surface - y;
        let material = if depth == 0 {
            if surface < self.sea_level {
                biome.soil
            } else {
                biome.surface
            }
        } else if depth <= biome.soil_depth {
            biome.soil
        } else {
            band.stone
        };
        shaded(material, x, y)
    }
}

fn shaded(material: MaterialId, x: i32, y: i32) -> Cell {
    let mut hasher = rustc_hash::FxHasher::default();
    (x, y).hash(&mut hasher);
    Cell::new(material, (hasher.finish() & 0xF) as u8)
}
