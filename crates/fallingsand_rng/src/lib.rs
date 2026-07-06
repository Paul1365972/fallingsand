const GOLDEN: u64 = 0x9e37_79b9_7f4a_7c15;

#[inline]
pub const fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[inline]
pub const fn pack(x: i32, y: i32) -> u64 {
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

    #[inline]
    pub const fn add(self, value: u64) -> Self {
        Hash(mix(self.0.wrapping_mul(GOLDEN).wrapping_add(value)))
    }

    #[inline]
    pub const fn pos(self, x: i32, y: i32) -> Self {
        self.add(pack(x, y))
    }

    #[inline]
    pub fn bytes(self, bytes: &[u8]) -> Self {
        let mut hash = self;
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            hash = hash.add(u64::from_le_bytes(chunk.try_into().unwrap()));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut buf = [0u8; 8];
            buf[..rest.len()].copy_from_slice(rest);
            hash = hash.add(u64::from_le_bytes(buf));
        }
        hash
    }

    #[inline]
    pub const fn stream(self) -> Stream {
        Stream(self.0)
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
        self.0 >> (64 - n)
    }

    #[inline]
    pub fn chance(self, chance: f32) -> bool {
        if chance.is_nan() || chance <= 0.0 {
            return false;
        }
        if chance >= 1.0 {
            return true;
        }
        self.0 < (f64::from(chance) * 2f64.powi(64)) as u64
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
        let span = (max - min) as u64 + 1;
        min + ((self.0 as u128 * span as u128) >> 64) as i32
    }
}

impl Default for Hash {
    #[inline]
    fn default() -> Self {
        Hash::new()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stream(u64);

impl Stream {
    #[inline]
    pub const fn new(seed: u64) -> Self {
        Stream(seed)
    }

    #[inline]
    pub fn draw(&mut self) -> Hash {
        self.0 = self.0.wrapping_add(GOLDEN);
        Hash(mix(self.0))
    }
}
