use crate::{TICK_DT, TICK_RATE};
use serde::{Deserialize, Serialize};

const FRAC_BITS: u32 = 10;
const SCALE: i64 = 1 << FRAC_BITS;

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Fixed(i64);

const fn round_div(n: i128, d: i128) -> i128 {
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

    pub const fn raw(self) -> i64 {
        self.0
    }

    pub const fn from_int(v: i32) -> Fixed {
        Fixed((v as i64) << FRAC_BITS)
    }

    pub const fn from_cell(cell: i32) -> Fixed {
        Fixed((cell as i64) << FRAC_BITS)
    }

    pub const fn cell_center(cell: i32) -> Fixed {
        Fixed(((cell as i64) << FRAC_BITS) + SCALE / 2)
    }

    pub const fn from_f32(v: f32) -> Fixed {
        let scaled = v * SCALE as f32;
        if scaled >= 0.0 {
            Fixed((scaled + 0.5) as i64)
        } else {
            Fixed((scaled - 0.5) as i64)
        }
    }

    pub const fn to_f32(self) -> f32 {
        self.0 as f32 / SCALE as f32
    }

    pub const fn vel_per_sec(v: f32) -> Fixed {
        Fixed::from_f32(v * TICK_DT)
    }

    pub const fn accel_per_sec2(a: f32) -> Fixed {
        Fixed::from_f32(a * TICK_DT * TICK_DT)
    }

    pub const fn vel_f32(self) -> f32 {
        self.to_f32() * TICK_RATE as f32
    }

    pub const fn floor_cell(self) -> i32 {
        (self.0 >> FRAC_BITS) as i32
    }

    pub const fn round_int(self) -> i32 {
        round_div(self.0 as i128, SCALE as i128) as i32
    }

    pub const fn max_cell(self) -> i32 {
        ((self.0 - 1) >> FRAC_BITS) as i32
    }

    pub const fn abs(self) -> Fixed {
        Fixed(self.0.abs())
    }

    pub const fn mul(self, rhs: Fixed) -> Fixed {
        Fixed(round_div(self.0 as i128 * rhs.0 as i128, SCALE as i128) as i64)
    }

    pub const fn mul_int(self, n: i32) -> Fixed {
        Fixed(self.0 * n as i64)
    }

    pub const fn per_substep(self, substeps: u32) -> Fixed {
        Fixed(round_div(self.0 as i128, substeps as i128) as i64)
    }

    pub const fn add_f32(self, d: f32) -> Fixed {
        Fixed(self.0 + Fixed::from_f32(d).0)
    }

    pub fn add_vel_f32(self, dv_per_sec: f32) -> Fixed {
        self + Fixed::vel_per_sec(dv_per_sec)
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
