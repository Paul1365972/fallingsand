use fallingsand_math::Hash;

const F2: f32 = 0.366_025_4;
const G2: f32 = 0.211_324_87;
const SIMPLEX_SCALE: f32 = 70.0;

const GRADIENTS: [(f32, f32); 8] = [
    (1.0, 1.0),
    (-1.0, 1.0),
    (1.0, -1.0),
    (-1.0, -1.0),
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
];

#[inline]
fn lattice(seed: u64, i: i32, j: i32) -> usize {
    let mut h = seed ^ ((i as u32 as u64) << 32) ^ (j as u32 as u64);
    h = (h ^ (h >> 29)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 32;
    (h & 7) as usize
}

#[inline]
fn corner(seed: u64, i: i32, j: i32, x: f32, y: f32) -> f32 {
    let attenuation = 0.5 - x * x - y * y;
    if attenuation <= 0.0 {
        return 0.0;
    }
    let (gx, gy) = GRADIENTS[lattice(seed, i, j)];
    let squared = attenuation * attenuation;
    squared * squared * (gx * x + gy * y)
}

fn simplex(seed: u64, x: f32, y: f32) -> f32 {
    let skew = (x + y) * F2;
    let i = (x + skew).floor();
    let j = (y + skew).floor();
    let unskew = (i + j) * G2;
    let x0 = x - (i - unskew);
    let y0 = y - (j - unskew);
    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
    let i = i as i32;
    let j = j as i32;

    let total = corner(seed, i, j, x0, y0)
        + corner(
            seed,
            i + i1,
            j + j1,
            x0 - i1 as f32 + G2,
            y0 - j1 as f32 + G2,
        )
        + corner(seed, i + 1, j + 1, x0 - 1.0 + 2.0 * G2, y0 - 1.0 + 2.0 * G2);
    (total * SIMPLEX_SCALE).clamp(-1.0, 1.0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Fbm,
    Ridged,
}

#[derive(Clone)]
pub struct Field {
    seed: u64,
    inv_period_x: f32,
    inv_period_y: f32,
    octaves: u32,
    lacunarity: f32,
    gain: f32,
    shape: Shape,
    normalization: f32,
}

impl Field {
    pub fn new(seed: u64, salt: Hash, period: f32) -> Self {
        Self {
            seed: Hash::seed(seed).salt(salt).get(),
            inv_period_x: 1.0 / period,
            inv_period_y: 1.0 / period,
            octaves: 1,
            lacunarity: 2.0,
            gain: 0.5,
            shape: Shape::Fbm,
            normalization: 1.0,
        }
    }

    pub fn octaves(mut self, octaves: u32) -> Self {
        self.octaves = octaves.max(1);
        self.renormalize()
    }

    pub fn gain(mut self, gain: f32) -> Self {
        self.gain = gain;
        self.renormalize()
    }

    pub fn flatten(mut self, factor: f32) -> Self {
        self.inv_period_y /= factor;
        self
    }

    pub fn ridged(mut self) -> Self {
        self.shape = Shape::Ridged;
        self.renormalize()
    }

    fn renormalize(mut self) -> Self {
        let mut total = 0.0;
        let mut amplitude = 1.0;
        for _ in 0..self.octaves {
            total += amplitude;
            amplitude *= self.gain;
        }
        self.normalization = 1.0 / total;
        self
    }

    pub fn at(&self, x: f32, y: f32) -> f32 {
        let mut sx = x * self.inv_period_x;
        let mut sy = y * self.inv_period_y;
        let mut amplitude = 1.0;
        let mut total = 0.0;
        let mut octave_seed = self.seed;
        for _ in 0..self.octaves {
            total += amplitude * simplex(octave_seed, sx, sy);
            sx *= self.lacunarity;
            sy *= self.lacunarity;
            amplitude *= self.gain;
            octave_seed = octave_seed
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                .rotate_left(17);
        }
        let total = total * self.normalization;
        match self.shape {
            Shape::Fbm => total,
            Shape::Ridged => 1.0 - total.abs(),
        }
    }
}
