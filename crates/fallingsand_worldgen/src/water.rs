use crate::biomes::{AQUIFER_MIN_DEPTH, AQUIFER_THRESHOLD, Band, Biome, SHALLOW_AQUIFER_FLOOR};
use crate::noise::{Field, noise_seed};
use fallingsand_core::MaterialId;
use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};

pub struct Waters {
    pub aquifer: Field,
    pub lava: Field,
}

impl Waters {
    pub fn new(seed: u64) -> Self {
        let mut aquifer = FastNoiseLite::with_seed(noise_seed(seed, "aquifer"));
        aquifer.set_noise_type(Some(NoiseType::OpenSimplex2S));
        aquifer.set_fractal_type(Some(FractalType::FBm));
        aquifer.set_fractal_octaves(Some(2));
        aquifer.set_frequency(Some(0.0035));

        let mut lava = FastNoiseLite::with_seed(noise_seed(seed, "lava"));
        lava.set_noise_type(Some(NoiseType::OpenSimplex2S));
        lava.set_fractal_type(Some(FractalType::FBm));
        lava.set_fractal_octaves(Some(2));
        lava.set_frequency(Some(0.015));

        Self {
            aquifer: Field::new(aquifer, None, 16),
            lava: Field::new(lava, None, 8),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cave_fill(
    biome: &Biome,
    band: &Band,
    lava_material: MaterialId,
    water_material: MaterialId,
    aquifer_value: f32,
    lava_value: f32,
    y: i32,
    depth: f32,
) -> Option<MaterialId> {
    if band.lava_pools && lava_value > band.lava_pool_threshold {
        return Some(lava_material);
    }
    if band.aquifers && depth > AQUIFER_MIN_DEPTH && aquifer_value > AQUIFER_THRESHOLD {
        let liquid = match biome.shallow_aquifer {
            Some(liquid) if y > SHALLOW_AQUIFER_FLOOR => liquid,
            _ => water_material,
        };
        return Some(liquid);
    }
    None
}
