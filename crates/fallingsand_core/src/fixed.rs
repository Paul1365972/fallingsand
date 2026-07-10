use crate::TICK_RATE;
use serde::{Deserialize, Serialize};

const FRAC_BITS: u32 = 8;
const SCALE: i32 = 1 << FRAC_BITS;

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Fixed(i32);

const fn round_div(n: i64, d: i64) -> i64 {
    let half = d / 2;
    if n >= 0 {
        (n + half) / d
    } else {
        (n - half) / d
    }
}

impl Fixed {
    pub const ZERO: Fixed = Fixed(0);
    pub const ONE: Fixed = Fixed(SCALE);
    pub const HALF: Fixed = Fixed(SCALE / 2);
    pub const SUBUNIT: Fixed = Fixed(1);

    pub const fn raw(self) -> i32 {
        self.0
    }

    pub const fn from_int(v: i32) -> Fixed {
        Fixed(v << FRAC_BITS)
    }

    pub const fn from_cell(cell: i32) -> Fixed {
        Fixed(cell << FRAC_BITS)
    }

    pub const fn cell_center(cell: i32) -> Fixed {
        Fixed((cell << FRAC_BITS) + SCALE / 2)
    }

    pub const fn from_f32(v: f32) -> Fixed {
        let scaled = v * SCALE as f32;
        if scaled >= 0.0 {
            Fixed((scaled + 0.5) as i32)
        } else {
            Fixed((scaled - 0.5) as i32)
        }
    }

    pub const fn to_f32(self) -> f32 {
        self.0 as f32 / SCALE as f32
    }

    pub const fn floor_cell(self) -> i32 {
        self.0 >> FRAC_BITS
    }

    pub const fn round_int(self) -> i32 {
        round_div(self.0 as i64, SCALE as i64) as i32
    }

    pub const fn max_cell(self) -> i32 {
        (self.0 - 1) >> FRAC_BITS
    }

    pub const fn abs(self) -> Fixed {
        Fixed(self.0.abs())
    }

    pub const fn mul(self, rhs: Fixed) -> Fixed {
        Fixed(round_div(self.0 as i64 * rhs.0 as i64, SCALE as i64) as i32)
    }

    pub const fn mul_int(self, n: i32) -> Fixed {
        Fixed(self.0 * n)
    }

    pub const fn per_tick(self) -> Fixed {
        Fixed(round_div(self.0 as i64, TICK_RATE as i64) as i32)
    }

    pub const fn per_substep(self, substeps: u32) -> Fixed {
        Fixed(round_div(self.0 as i64, (TICK_RATE * substeps) as i64) as i32)
    }

    pub const fn add_f32(self, d: f32) -> Fixed {
        Fixed(self.0 + Fixed::from_f32(d).0)
    }
}

impl std::ops::Add for Fixed {
    type Output = Fixed;

    fn add(self, rhs: Fixed) -> Fixed {
        Fixed(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Fixed {
    type Output = Fixed;

    fn sub(self, rhs: Fixed) -> Fixed {
        Fixed(self.0 - rhs.0)
    }
}

impl std::ops::Neg for Fixed {
    type Output = Fixed;

    fn neg(self) -> Fixed {
        Fixed(-self.0)
    }
}

impl std::ops::AddAssign for Fixed {
    fn add_assign(&mut self, rhs: Fixed) {
        self.0 += rhs.0;
    }
}

impl std::ops::SubAssign for Fixed {
    fn sub_assign(&mut self, rhs: Fixed) {
        self.0 -= rhs.0;
    }
}
