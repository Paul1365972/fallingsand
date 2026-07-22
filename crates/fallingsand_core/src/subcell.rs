use fallingsand_math::{SUBCELL_BITS, SUBCELL_UNITS_PER_CELL, TICK_DT, TICK_RATE};
use serde::{Deserialize, Serialize};

const UNITS_PER_CELL: i64 = SUBCELL_UNITS_PER_CELL as i64;

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Subcell(i64);

const fn round_div(n: i128, d: i128) -> i128 {
    let half = d / 2;
    if n >= 0 {
        (n + half) / d
    } else {
        (n - half) / d
    }
}

impl Subcell {
    pub const ZERO: Self = Self(0);
    pub const QUANTUM: Self = Self(1);

    pub const fn raw(self) -> i64 {
        self.0
    }

    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    pub const fn from_cell(cell: i32) -> Self {
        Self((cell as i64) << SUBCELL_BITS)
    }

    pub const fn cell_center(cell: i32) -> Self {
        Self(((cell as i64) << SUBCELL_BITS) + UNITS_PER_CELL / 2)
    }

    pub const fn from_cells(v: f32) -> Self {
        let scaled = v * UNITS_PER_CELL as f32;
        if scaled >= 0.0 {
            Self((scaled + 0.5) as i64)
        } else {
            Self((scaled - 0.5) as i64)
        }
    }

    pub const fn to_cells(self) -> f32 {
        self.0 as f32 / UNITS_PER_CELL as f32
    }

    pub const fn from_cells_per_second(v: f32) -> Self {
        Self::from_cells(v * TICK_DT)
    }

    pub const fn from_cells_per_second_squared(a: i32) -> Self {
        Self(round_div(
            a as i128 * UNITS_PER_CELL as i128,
            TICK_RATE as i128 * TICK_RATE as i128,
        ) as i64)
    }

    pub const fn to_cells_per_second(self) -> f32 {
        self.to_cells() * TICK_RATE as f32
    }

    pub const fn floor_cell(self) -> i32 {
        (self.0 >> SUBCELL_BITS) as i32
    }

    pub const fn round_cells(self) -> i32 {
        round_div(self.0 as i128, UNITS_PER_CELL as i128) as i32
    }

    pub const fn max_cell(self) -> i32 {
        ((self.0 - 1) >> SUBCELL_BITS) as i32
    }

    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }

    pub const fn scaled_by(self, factor: f32) -> Self {
        let scaled = self.0 as f64 * factor as f64;
        if scaled >= 0.0 {
            Self((scaled + 0.5) as i64)
        } else {
            Self((scaled - 0.5) as i64)
        }
    }

    pub const fn times(self, n: i32) -> Self {
        Self(self.0 * n as i64)
    }

    pub const fn per_substep(self, substeps: u32) -> Self {
        Self(round_div(self.0 as i128, substeps as i128) as i64)
    }

    pub const fn add_cells(self, cells: f32) -> Self {
        Self(self.0 + Self::from_cells(cells).0)
    }

    pub fn add_cells_per_second(self, velocity: f32) -> Self {
        self + Self::from_cells_per_second(velocity)
    }
}

impl std::ops::Add for Subcell {
    type Output = Subcell;

    fn add(self, rhs: Subcell) -> Subcell {
        Subcell(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Subcell {
    type Output = Subcell;

    fn sub(self, rhs: Subcell) -> Subcell {
        Subcell(self.0 - rhs.0)
    }
}

impl std::ops::Neg for Subcell {
    type Output = Subcell;

    fn neg(self) -> Subcell {
        Subcell(-self.0)
    }
}

impl std::ops::AddAssign for Subcell {
    fn add_assign(&mut self, rhs: Subcell) {
        self.0 += rhs.0;
    }
}

impl std::ops::SubAssign for Subcell {
    fn sub_assign(&mut self, rhs: Subcell) {
        self.0 -= rhs.0;
    }
}
