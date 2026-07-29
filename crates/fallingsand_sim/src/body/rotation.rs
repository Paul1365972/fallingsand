const SHEAR_BITS: u32 = 16;
const SHEAR_SCALE: i64 = 1 << SHEAR_BITS;
const SHEAR_CENTER: i32 = 32;
const CELL: i64 = fallingsand_math::SUBCELL_UNITS_PER_CELL as i64;
const SHEARS: [(i64, i64); 65] = [
    (-27146, -46341),
    (-26208, -45190),
    (-25280, -44011),
    (-24360, -42806),
    (-23449, -41576),
    (-22546, -40320),
    (-21650, -39040),
    (-20762, -37736),
    (-19880, -36410),
    (-19005, -35062),
    (-18136, -33692),
    (-17273, -32303),
    (-16416, -30893),
    (-15564, -29466),
    (-14717, -28020),
    (-13874, -26558),
    (-13036, -25080),
    (-12202, -23586),
    (-11372, -22078),
    (-10545, -20557),
    (-9721, -19024),
    (-8901, -17479),
    (-8083, -15924),
    (-7268, -14359),
    (-6455, -12785),
    (-5644, -11204),
    (-4834, -9616),
    (-4026, -8022),
    (-3220, -6424),
    (-2414, -4821),
    (-1609, -3216),
    (-804, -1608),
    (0, 0),
    (804, 1608),
    (1609, 3216),
    (2414, 4821),
    (3220, 6424),
    (4026, 8022),
    (4834, 9616),
    (5644, 11204),
    (6455, 12785),
    (7268, 14359),
    (8083, 15924),
    (8901, 17479),
    (9721, 19024),
    (10545, 20557),
    (11372, 22078),
    (12202, 23586),
    (13036, 25080),
    (13874, 26558),
    (14717, 28020),
    (15564, 29466),
    (16416, 30893),
    (17273, 32303),
    (18136, 33692),
    (19005, 35062),
    (19880, 36410),
    (20762, 37736),
    (21650, 39040),
    (22546, 40320),
    (23449, 41576),
    (24360, 42806),
    (25280, 44011),
    (26208, 45190),
    (27146, 46341),
];

pub(super) const ANGLE_STEPS: u32 = 256;
pub(super) const TURN_UNITS: i64 = 1 << 20;
pub(super) const ORIENTATION_UNITS: i64 = TURN_UNITS / ANGLE_STEPS as i64;

pub(super) const TAU_NUMERATOR: i128 = 710;
const TAU_DENOMINATOR: i128 = 113;
pub(super) const RADIANS_PER_TURN: i128 = TURN_UNITS as i128 * TAU_DENOMINATOR;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub(super) struct Spin(i64);

impl Spin {
    pub(super) const ZERO: Self = Self(0);

    pub(super) const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    pub(super) const fn raw(self) -> i64 {
        self.0
    }

    pub(super) fn speed_at(self, lever: i64) -> i64 {
        round_div_i128(
            TAU_NUMERATOR * i128::from(self.0) * i128::from(lever),
            RADIANS_PER_TURN,
        ) as i64
    }

    pub(super) fn for_speed_at(speed: i64, lever: i64) -> Self {
        Self(round_div_i128(
            i128::from(speed) * RADIANS_PER_TURN,
            TAU_NUMERATOR * i128::from(lever.max(1)),
        ) as i64)
    }

    pub(super) fn from_angular_impulse(impulse: i128, moment: i128) -> Self {
        if moment == 0 {
            return Self::ZERO;
        }
        Self((impulse * RADIANS_PER_TURN / (moment * TAU_NUMERATOR)) as i64)
    }

    pub(super) fn clamped(self, limit: Self) -> Self {
        Self(self.0.clamp(-limit.0, limit.0))
    }
}

impl std::ops::Add for Spin {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::AddAssign for Spin {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

fn decompose(step: u32) -> (u32, i32) {
    let quarter = ANGLE_STEPS as i32 / 4;
    let quarters =
        ((step as i64 * 4 + ANGLE_STEPS as i64 / 2).div_euclid(ANGLE_STEPS as i64)) as i32;
    let residual_steps = step as i32 - quarters * quarter;
    (quarters.rem_euclid(4) as u32, residual_steps)
}

fn residual_shears(residual_steps: i32) -> (i64, i64) {
    SHEARS[(residual_steps + SHEAR_CENTER) as usize]
}

fn round_shear(coefficient: i64, coordinate: i64, center: i64) -> i64 {
    round_div_i128(
        i128::from(coefficient) * (i128::from(coordinate) * i128::from(CELL) - i128::from(center)),
        i128::from(SHEAR_SCALE) * i128::from(CELL),
    ) as i64
}

fn round_div_i128(numer: i128, denominator: i128) -> i128 {
    let half = denominator / 2;
    if numer >= 0 {
        (numer + half) / denominator
    } else {
        (numer - half) / denominator
    }
}

pub(super) fn rotate_offset(step: u32, local_com: (i64, i64), dx: i32, dy: i32) -> (i32, i32) {
    let (quarters, residual_steps) = decompose(step);
    let (mut x, mut y) = (i64::from(dx), i64::from(dy));
    let (mut center_x, mut center_y) = local_com;
    for _ in 0..quarters {
        (x, y) = (-y, x);
        (center_x, center_y) = (-center_y, center_x);
    }
    let (t, s) = residual_shears(residual_steps);
    x -= round_shear(t, y, center_y);
    y += round_shear(s, x, center_x);
    x -= round_shear(t, y, center_y);
    (x as i32, y as i32)
}
