use crate::biomes::{
    Band, CAVE_SURFACE_GATE, CAVERN_CHEESE_THRESHOLD, CAVERN_MIN_DEPTH, CAVERN_RARITY_THRESHOLD,
    SHAFT_WIDTH,
};
use crate::noise::{Field, noise_seed};
use fastnoise_lite::{DomainWarpType, FastNoiseLite, FractalType, NoiseType};

pub struct Caves {
    pub tunnel_a: Field,
    pub tunnel_b: Field,
    pub cheese: Field,
    pub rarity: Field,
    shaft: FastNoiseLite,
}

impl Caves {
    pub fn new(seed: u64) -> Self {
        let warp = |purpose: &str| {
            let mut warp = FastNoiseLite::with_seed(noise_seed(seed, purpose));
            warp.set_domain_warp_type(Some(DomainWarpType::OpenSimplex2));
            warp.set_domain_warp_amp(Some(60.0));
            warp.set_frequency(Some(0.006));
            warp.set_fractal_type(Some(FractalType::DomainWarpProgressive));
            warp.set_fractal_octaves(Some(2));
            warp
        };
        let tunnel = |purpose: &str, frequency: f32| {
            let mut noise = FastNoiseLite::with_seed(noise_seed(seed, purpose));
            noise.set_noise_type(Some(NoiseType::OpenSimplex2S));
            noise.set_frequency(Some(frequency));
            noise
        };

        let mut cheese = FastNoiseLite::with_seed(noise_seed(seed, "cheese"));
        cheese.set_noise_type(Some(NoiseType::OpenSimplex2S));
        cheese.set_fractal_type(Some(FractalType::FBm));
        cheese.set_fractal_octaves(Some(3));
        cheese.set_frequency(Some(0.004));

        let mut rarity = FastNoiseLite::with_seed(noise_seed(seed, "cavern_rarity"));
        rarity.set_noise_type(Some(NoiseType::OpenSimplex2));
        rarity.set_frequency(Some(0.0007));

        let mut shaft = FastNoiseLite::with_seed(noise_seed(seed, "shaft"));
        shaft.set_noise_type(Some(NoiseType::OpenSimplex2));
        shaft.set_frequency(Some(0.0013));

        Self {
            tunnel_a: Field::new(tunnel("tunnel_a", 0.006), Some(warp("tunnel_a_warp")), 4),
            tunnel_b: Field::new(tunnel("tunnel_b", 0.0075), Some(warp("tunnel_b_warp")), 4),
            cheese: Field::new(cheese, None, 4),
            rarity: Field::new(rarity, None, 32),
            shaft,
        }
    }

    pub fn shaft_factor(&self, x: i32) -> f32 {
        let value = self.shaft.get_noise_2d(x as f32, 0.0).abs();
        (1.0 - value / SHAFT_WIDTH).max(0.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CaveSample {
    pub tunnel_a: f32,
    pub tunnel_b: f32,
    pub cheese: f32,
    pub rarity: f32,
}

pub fn carved(sample: CaveSample, depth: f32, band: &Band, shaft: f32, canyon: f32) -> bool {
    let opening = shaft.max(canyon * 0.7);
    let gate = CAVE_SURFACE_GATE * (1.0 - opening);
    if depth <= gate.max(0.0) {
        return false;
    }
    let widen = 1.0 + opening * 0.8 * (1.0 - (depth / 60.0).min(1.0));
    let threshold =
        (band.cave_threshold + band.cave_depth_bonus * (depth / 400.0).min(1.0)) * widen;
    if sample.tunnel_a.abs() < threshold && sample.tunnel_b.abs() < threshold {
        return true;
    }
    depth > CAVERN_MIN_DEPTH
        && sample.rarity > CAVERN_RARITY_THRESHOLD
        && sample.cheese > CAVERN_CHEESE_THRESHOLD
}
