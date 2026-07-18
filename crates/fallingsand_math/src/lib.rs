pub const TICK_RATE: u32 = 60;
pub const TICK_DT: f32 = 1.0 / TICK_RATE as f32;
pub const SUBCELL_BITS: u32 = 10;
pub const SUBCELL_UNITS_PER_CELL: i32 = 1 << SUBCELL_BITS;

pub const fn ticks_from_secs(secs: f32) -> u64 {
    (secs * TICK_RATE as f32 + 0.5) as u64
}

const GOLDEN: u64 = 0x9e37_79b9_7f4a_7c15;

#[inline]
pub(crate) const fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[inline]
pub(crate) const fn pack(x: i32, y: i32) -> u64 {
    ((x as u32 as u64) << 32) | y as u32 as u64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hash(u64);

impl Hash {
    #[inline]
    pub const fn new() -> Self {
        Hash(0)
    }

    #[inline]
    pub const fn seed(seed: u64) -> Self {
        Hash(seed)
    }

    pub const fn label(label: &str) -> Self {
        let bytes = label.as_bytes();
        let mut hash = Self::new();
        let mut index = 0;
        while index < bytes.len() {
            hash = hash.add(bytes[index] as u64);
            index += 1;
        }
        hash
    }

    #[inline]
    pub const fn add(self, value: u64) -> Self {
        Hash(mix(self.0.wrapping_mul(GOLDEN).wrapping_add(value)))
    }

    #[inline]
    pub const fn salt(self, salt: Self) -> Self {
        self.add(salt.0)
    }

    #[inline]
    pub const fn pos(self, x: i32, y: i32) -> Self {
        self.add(pack(x, y))
    }

    #[inline]
    pub const fn rng(self) -> Rng {
        Rng(self.0)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn bit(self) -> bool {
        self.0 >> 63 != 0
    }

    #[inline]
    pub const fn bits(self, n: u32) -> u64 {
        if n == 0 {
            0
        } else if n >= 64 {
            self.0
        } else {
            self.0 >> (64 - n)
        }
    }

    #[inline]
    pub const fn below(self, threshold: u64) -> bool {
        threshold == u64::MAX || self.0 < threshold
    }

    #[inline]
    pub fn chance(self, chance: f32) -> bool {
        self.below(chance_threshold(chance))
    }

    #[inline]
    pub fn unit(self) -> f32 {
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    #[inline]
    pub fn range(self, min: i32, max: i32) -> i32 {
        if max <= min {
            return min;
        }
        let span = (max as i64 - min as i64) as u64 + 1;
        (min as i64 + ((self.0 as u128 * span as u128) >> 64) as i64) as i32
    }
}

impl Default for Hash {
    #[inline]
    fn default() -> Self {
        Hash::new()
    }
}

pub fn chance_threshold(chance: f32) -> u64 {
    if chance.is_nan() || chance <= 0.0 {
        return 0;
    }
    if chance >= 1.0 {
        return u64::MAX;
    }
    (f64::from(chance) * 2f64.powi(64)) as u64
}

#[derive(Clone, Debug, Default)]
pub struct Rng(u64);

impl Rng {
    #[inline]
    pub fn draw(&mut self) -> Hash {
        self.0 = self.0.wrapping_add(GOLDEN);
        Hash(mix(self.0))
    }
}
