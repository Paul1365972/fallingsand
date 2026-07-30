use crate::chunk::CHUNK_SIZE;
use crate::coords::{CellPos, ChunkPos};
use serde::{Deserialize, Serialize};

pub const FOG_TEXEL_BITS: u32 = 2;
pub const FOG_TEXEL_CELLS: usize = 1 << FOG_TEXEL_BITS;
pub const FOG_CHUNK_SIDE: usize = CHUNK_SIZE / FOG_TEXEL_CELLS;
pub const FOG_CHUNK_TEXELS: usize = FOG_CHUNK_SIDE * FOG_CHUNK_SIDE;
pub const FOG_CHUNK_BYTES: usize = FOG_CHUNK_TEXELS / 8;

const _: () = assert!(FOG_CHUNK_SIDE.is_power_of_two());
const FOG_CHUNK_BITS: u32 = FOG_CHUNK_SIDE.trailing_zeros();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FogMask([u8; FOG_CHUNK_BYTES]);

impl Default for FogMask {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl FogMask {
    pub const EMPTY: Self = Self([0; FOG_CHUNK_BYTES]);
    pub const FULL: Self = Self([u8::MAX; FOG_CHUNK_BYTES]);

    pub const fn from_bytes(bytes: [u8; FOG_CHUNK_BYTES]) -> Self {
        Self(bytes)
    }

    pub const fn bytes(&self) -> &[u8; FOG_CHUNK_BYTES] {
        &self.0
    }

    #[inline]
    pub const fn get(&self, index: usize) -> bool {
        self.0[index >> 3] & (1 << (index & 7)) != 0
    }

    #[inline]
    pub fn set(&mut self, index: usize) {
        self.0[index >> 3] |= 1 << (index & 7);
    }

    #[inline]
    pub fn assign(&mut self, index: usize, value: bool) {
        let bit = 1 << (index & 7);
        if value {
            self.0[index >> 3] |= bit;
        } else {
            self.0[index >> 3] &= !bit;
        }
    }

    pub fn union_in_place(&mut self, other: &Self) {
        for (slot, added) in self.0.iter_mut().zip(other.0) {
            *slot |= added;
        }
    }

    pub fn is_full(&self) -> bool {
        self.0.iter().all(|byte| *byte == u8::MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FogPos {
    pub x: i32,
    pub y: i32,
}

impl FogPos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub const fn of(pos: CellPos) -> Self {
        Self {
            x: pos.x >> FOG_TEXEL_BITS,
            y: pos.y >> FOG_TEXEL_BITS,
        }
    }

    pub const fn translated(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }

    pub const fn chunk(self) -> ChunkPos {
        ChunkPos::new(self.x >> FOG_CHUNK_BITS, self.y >> FOG_CHUNK_BITS)
    }

    pub const fn base_cell(self) -> CellPos {
        CellPos::new(
            self.x.wrapping_shl(FOG_TEXEL_BITS),
            self.y.wrapping_shl(FOG_TEXEL_BITS),
        )
    }

    pub fn in_chunk(chunk: ChunkPos) -> impl Iterator<Item = (usize, Self)> {
        let base = Self::new(
            chunk.x.wrapping_shl(FOG_CHUNK_BITS),
            chunk.y.wrapping_shl(FOG_CHUNK_BITS),
        );
        (0..FOG_CHUNK_TEXELS).map(move |index| {
            let texel = base.translated(
                (index % FOG_CHUNK_SIDE) as i32,
                (index / FOG_CHUNK_SIDE) as i32,
            );
            (index, texel)
        })
    }
}
