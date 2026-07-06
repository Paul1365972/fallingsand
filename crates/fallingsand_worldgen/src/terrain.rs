use crate::biomes::{POND_ANCHOR_GRID, WorldDef};
use crate::noise::{Field, hash1, sub_seed};
use fastnoise_lite::{DomainWarpType, FastNoiseLite, FractalType, NoiseType};

pub struct Terrain {
    seed: u64,
    continent: FastNoiseLite,
    hills: FastNoiseLite,
    detail: FastNoiseLite,
    mesa: FastNoiseLite,
    canyon: FastNoiseLite,
    river: FastNoiseLite,
    mountain_mask: FastNoiseLite,
    ridge: FastNoiseLite,
    band_edge: FastNoiseLite,
    pub shape: Field,
}

const CONTINENT_AMPLITUDE: f32 = 210.0;
const BASE_HEIGHT: f32 = 12.0;
const DETAIL_AMPLITUDE: f32 = 3.0;
const TERRACE_STEP: f32 = 26.0;
const CANYON_WIDTH: f32 = 0.09;
const CANYON_DEPTH: f32 = 110.0;
const RIVER_WIDTH: f32 = 0.028;
const MOUNTAIN_AMPLITUDE: f32 = 260.0;
const MOUNTAIN_MASK_START: f32 = 0.18;
const BIOME_CELL: i32 = 1400;
const BIOME_BLEND_CELLS: i32 = 64;

impl Terrain {
    pub fn new(seed: u64) -> Self {
        let mut continent = FastNoiseLite::with_seed(sub_seed(seed, "continent"));
        continent.set_noise_type(Some(NoiseType::OpenSimplex2));
        continent.set_fractal_type(Some(FractalType::FBm));
        continent.set_fractal_octaves(Some(3));
        continent.set_frequency(Some(0.0006));

        let mut hills = FastNoiseLite::with_seed(sub_seed(seed, "hills"));
        hills.set_noise_type(Some(NoiseType::OpenSimplex2));
        hills.set_fractal_type(Some(FractalType::FBm));
        hills.set_fractal_octaves(Some(4));
        hills.set_frequency(Some(0.004));

        let mut detail = FastNoiseLite::with_seed(sub_seed(seed, "detail"));
        detail.set_noise_type(Some(NoiseType::OpenSimplex2));
        detail.set_frequency(Some(0.035));

        let mut mesa = FastNoiseLite::with_seed(sub_seed(seed, "mesa"));
        mesa.set_noise_type(Some(NoiseType::OpenSimplex2));
        mesa.set_frequency(Some(0.0016));

        let mut canyon = FastNoiseLite::with_seed(sub_seed(seed, "canyon"));
        canyon.set_noise_type(Some(NoiseType::OpenSimplex2));
        canyon.set_frequency(Some(0.0008));

        let mut river = FastNoiseLite::with_seed(sub_seed(seed, "river"));
        river.set_noise_type(Some(NoiseType::OpenSimplex2));
        river.set_frequency(Some(0.0007));

        let mut mountain_mask = FastNoiseLite::with_seed(sub_seed(seed, "mountain_mask"));
        mountain_mask.set_noise_type(Some(NoiseType::OpenSimplex2));
        mountain_mask.set_frequency(Some(0.00025));

        let mut ridge = FastNoiseLite::with_seed(sub_seed(seed, "ridge"));
        ridge.set_noise_type(Some(NoiseType::OpenSimplex2));
        ridge.set_fractal_type(Some(FractalType::FBm));
        ridge.set_fractal_octaves(Some(2));
        ridge.set_frequency(Some(0.0025));

        let mut band_edge = FastNoiseLite::with_seed(sub_seed(seed, "band_edge"));
        band_edge.set_noise_type(Some(NoiseType::OpenSimplex2));
        band_edge.set_frequency(Some(0.004));

        let mut shape = FastNoiseLite::with_seed(sub_seed(seed, "shape"));
        shape.set_noise_type(Some(NoiseType::OpenSimplex2S));
        shape.set_fractal_type(Some(FractalType::FBm));
        shape.set_fractal_octaves(Some(3));
        shape.set_frequency(Some(0.02));
        let mut shape_warp = FastNoiseLite::with_seed(sub_seed(seed, "shape_warp"));
        shape_warp.set_domain_warp_type(Some(DomainWarpType::OpenSimplex2));
        shape_warp.set_domain_warp_amp(Some(30.0));
        shape_warp.set_frequency(Some(0.01));

        Self {
            seed,
            continent,
            hills,
            detail,
            mesa,
            canyon,
            river,
            mountain_mask,
            ridge,
            band_edge,
            shape: Field::new(shape, Some(shape_warp), 4),
        }
    }

    fn biome_of_cell(&self, count: usize, cell: i32) -> usize {
        (hash1(self.seed, "biome_cell", cell) % count as u64) as usize
    }

    pub fn biome_mix(&self, count: usize, x: i32) -> (usize, usize, f32) {
        let cell = x.div_euclid(BIOME_CELL);
        let offset = x - cell * BIOME_CELL;
        let own = self.biome_of_cell(count, cell);
        if offset < BIOME_BLEND_CELLS {
            let left = self.biome_of_cell(count, cell - 1);
            let mix = 0.5 + offset as f32 / (2.0 * BIOME_BLEND_CELLS as f32);
            (left, own, mix.clamp(0.5, 1.0))
        } else if offset >= BIOME_CELL - BIOME_BLEND_CELLS {
            let right = self.biome_of_cell(count, cell + 1);
            let mix = (offset - (BIOME_CELL - BIOME_BLEND_CELLS)) as f32
                / (2.0 * BIOME_BLEND_CELLS as f32);
            (own, right, mix.clamp(0.0, 0.5))
        } else {
            (own, own, 0.0)
        }
    }

    pub fn biome_at(&self, count: usize, x: i32) -> usize {
        let (a, b, mix) = self.biome_mix(count, x);
        if mix < 0.5 { a } else { b }
    }

    pub fn ruggedness(&self, def: &WorldDef, x: i32) -> f32 {
        let (a, b, mix) = self.biome_mix(def.biomes.len(), x);
        def.biomes[a].ruggedness * (1.0 - mix) + def.biomes[b].ruggedness * mix
    }

    pub fn surface_height(&self, def: &WorldDef, x: i32) -> i32 {
        let (a, b, mix) = self.biome_mix(def.biomes.len(), x);
        let amplitude =
            def.biomes[a].height_amplitude * (1.0 - mix) + def.biomes[b].height_amplitude * mix;
        let fx = x as f32;
        let continent = self.continent.get_noise_2d(fx, 0.0) * CONTINENT_AMPLITUDE;
        let hills = self.hills.get_noise_2d(fx, 100.0) * amplitude;
        let detail = self.detail.get_noise_2d(fx, 200.0) * DETAIL_AMPLITUDE;
        let mut height = BASE_HEIGHT + continent + hills + detail;

        let mask = self.mountain_mask.get_noise_2d(fx, 600.0);
        if mask > MOUNTAIN_MASK_START {
            let strength = ((mask - MOUNTAIN_MASK_START) / 0.5).clamp(0.0, 1.0);
            let ridge = 1.0 - self.ridge.get_noise_2d(fx, 700.0).abs();
            height += strength * ridge * ridge * MOUNTAIN_AMPLITUDE;
        }

        let mesa = self.mesa.get_noise_2d(fx, 300.0);
        if mesa > 0.25 {
            let strength = ((mesa - 0.25) / 0.2).clamp(0.0, 1.0);
            let terraced = (height / TERRACE_STEP).round() * TERRACE_STEP;
            height += (terraced - height) * strength;
        }

        let canyon = self.canyon.get_noise_2d(fx, 400.0).abs();
        if canyon < CANYON_WIDTH {
            let profile = 1.0 - canyon / CANYON_WIDTH;
            height -= profile * profile * CANYON_DEPTH;
        }

        let river = self.river.get_noise_2d(fx, 500.0).abs();
        if river < RIVER_WIDTH {
            let blend = 1.0 - river / RIVER_WIDTH;
            let smooth = blend * blend * (3.0 - 2.0 * blend);
            let target = (def.sea_level - 4) as f32;
            height += (target - height) * smooth;
        }

        height.round() as i32
    }

    pub fn canyon_factor(&self, x: i32) -> f32 {
        let canyon = self.canyon.get_noise_2d(x as f32, 400.0).abs();
        (1.0 - canyon / CANYON_WIDTH).max(0.0)
    }

    pub fn band_jitter(&self, x: i32, index: usize) -> f32 {
        self.band_edge.get_noise_2d(x as f32, index as f32 * 1000.0) * 24.0
    }

    pub fn pond(&self, def: &WorldDef, x: i32, biome: usize) -> Option<(i32, i32)> {
        let chance = def.biomes[biome].pond_chance;
        if chance <= 0.0 {
            return None;
        }
        let cell = x.div_euclid(POND_ANCHOR_GRID);
        for anchor in [cell - 1, cell, cell + 1] {
            let hash = hash1(self.seed, "pond", anchor);
            if (hash & 0xFF) as f32 >= chance * 256.0 {
                continue;
            }
            let center = anchor * POND_ANCHOR_GRID
                + POND_ANCHOR_GRID / 4
                + ((hash >> 8) % (POND_ANCHOR_GRID / 2) as u64) as i32;
            let radius = 10 + ((hash >> 24) % 13) as i32;
            let dx = x - center;
            if dx.abs() > radius {
                continue;
            }
            if self.biome_at(def.biomes.len(), center) != biome {
                continue;
            }
            let center_surface = self.surface_height(def, center);
            if center_surface <= def.sea_level + 2 {
                continue;
            }
            let depth = 3 + ((hash >> 40) % 5) as i32;
            let profile = 1.0 - (dx as f32 / radius as f32).powi(2);
            let floor = center_surface - 2 - (depth as f32 * profile).round() as i32;
            let level = center_surface - 1;
            return Some((floor, level));
        }
        None
    }
}
