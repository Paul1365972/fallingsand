use fastnoise_lite::FastNoiseLite;
use std::hash::{Hash, Hasher};

pub fn sub_seed(seed: u64, purpose: &str) -> i32 {
    let mut hasher = rustc_hash::FxHasher::default();
    (seed, purpose).hash(&mut hasher);
    hasher.finish() as i32
}

pub fn hash2(seed: u64, purpose: &str, x: i32, y: i32) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    (seed, purpose, x, y).hash(&mut hasher);
    hasher.finish()
}

pub fn hash1(seed: u64, purpose: &str, x: i32) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    (seed, purpose, x).hash(&mut hasher);
    hasher.finish()
}

pub struct Xorshift(u64);

impl Xorshift {
    pub fn new(state: u64) -> Self {
        Self(state.max(1))
    }

    pub fn step(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn unit(&mut self) -> f32 {
        (self.step() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, min: i32, max: i32) -> i32 {
        min + (self.step() % (max - min + 1).max(1) as u64) as i32
    }
}

pub struct Field {
    noise: FastNoiseLite,
    warp: Option<FastNoiseLite>,
    step: i32,
}

impl Field {
    pub fn new(noise: FastNoiseLite, warp: Option<FastNoiseLite>, step: i32) -> Self {
        Self { noise, warp, step }
    }

    pub fn corner(&self, cx: i32, cy: i32) -> f32 {
        let (x, y) = (cx as f32, cy as f32);
        let (wx, wy) = match &self.warp {
            Some(warp) => warp.domain_warp_2d(x, y),
            None => (x, y),
        };
        self.noise.get_noise_2d(wx, wy)
    }

    pub fn at(&self, x: i32, y: i32) -> f32 {
        let step = self.step;
        let cx = x.div_euclid(step) * step;
        let cy = y.div_euclid(step) * step;
        let fx = (x - cx) as f32 / step as f32;
        let fy = (y - cy) as f32 / step as f32;
        let c00 = self.corner(cx, cy);
        let c10 = self.corner(cx + step, cy);
        let c01 = self.corner(cx, cy + step);
        let c11 = self.corner(cx + step, cy + step);
        bilerp(c00, c10, c01, c11, fx, fy)
    }
}

fn bilerp(c00: f32, c10: f32, c01: f32, c11: f32, fx: f32, fy: f32) -> f32 {
    let top = c00 + (c10 - c00) * fx;
    let bottom = c01 + (c11 - c01) * fx;
    top + (bottom - top) * fy
}

pub struct Cached<'f> {
    field: &'f Field,
    base_x: i32,
    base_y: i32,
    columns: usize,
    values: Vec<f32>,
}

impl<'f> Cached<'f> {
    pub fn build(field: &'f Field, min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Self {
        let step = field.step;
        let base_x = min_x.div_euclid(step) * step;
        let base_y = min_y.div_euclid(step) * step;
        let columns = ((max_x - base_x).div_euclid(step) + 2) as usize;
        let rows = ((max_y - base_y).div_euclid(step) + 2) as usize;
        let mut values = Vec::with_capacity(columns * rows);
        for row in 0..rows {
            for column in 0..columns {
                values
                    .push(field.corner(base_x + column as i32 * step, base_y + row as i32 * step));
            }
        }
        Self {
            field,
            base_x,
            base_y,
            columns,
            values,
        }
    }

    pub fn at(&self, x: i32, y: i32) -> f32 {
        let step = self.field.step;
        let column = (x - self.base_x).div_euclid(step);
        let row = (y - self.base_y).div_euclid(step);
        if column < 0
            || row < 0
            || (column + 1) as usize >= self.columns
            || ((row + 1) as usize + 1) * self.columns > self.values.len()
        {
            return self.field.at(x, y);
        }
        let (column, row) = (column as usize, row as usize);
        let cx = self.base_x + column as i32 * step;
        let cy = self.base_y + row as i32 * step;
        let fx = (x - cx) as f32 / step as f32;
        let fy = (y - cy) as f32 / step as f32;
        let c00 = self.values[row * self.columns + column];
        let c10 = self.values[row * self.columns + column + 1];
        let c01 = self.values[(row + 1) * self.columns + column];
        let c11 = self.values[(row + 1) * self.columns + column + 1];
        bilerp(c00, c10, c01, c11, fx, fy)
    }
}
