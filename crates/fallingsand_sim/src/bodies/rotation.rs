const SHEAR_BITS: u32 = 16;
const SHEAR_SCALE: i64 = 1 << SHEAR_BITS;
const SHEARS_128: [(i64, i64); 33] = [
    (-27146, -46341),
    (-25280, -44011),
    (-23449, -41576),
    (-21650, -39040),
    (-19880, -36410),
    (-18136, -33692),
    (-16416, -30893),
    (-14717, -28020),
    (-13036, -25080),
    (-11372, -22078),
    (-9721, -19024),
    (-8083, -15924),
    (-6455, -12785),
    (-4834, -9616),
    (-3220, -6424),
    (-1609, -3216),
    (0, 0),
    (1609, 3216),
    (3220, 6424),
    (4834, 9616),
    (6455, 12785),
    (8083, 15924),
    (9721, 19024),
    (11372, 22078),
    (13036, 25080),
    (14717, 28020),
    (16416, 30893),
    (18136, 33692),
    (19880, 36410),
    (21650, 39040),
    (23449, 41576),
    (25280, 44011),
    (27146, 46341),
];

const SHEAR_STEPS: u32 = 128;
pub(super) const ANGLE_STEPS: u32 = 64;
pub(super) const TURN_UNITS: i64 = 1 << 20;

pub(super) fn quantize_step(angle: i64) -> u32 {
    round_div_i128(
        angle as i128 * i128::from(ANGLE_STEPS),
        i128::from(TURN_UNITS),
    )
    .rem_euclid(i128::from(ANGLE_STEPS)) as u32
}

fn decompose(step: u32) -> (u32, i32) {
    let quarter = ANGLE_STEPS as i32 / 4;
    let quarters =
        ((step as i64 * 4 + ANGLE_STEPS as i64 / 2).div_euclid(ANGLE_STEPS as i64)) as i32;
    let residual_steps = step as i32 - quarters * quarter;
    (quarters.rem_euclid(4) as u32, residual_steps)
}

fn residual_shears(residual_steps: i32) -> (i64, i64) {
    let index = residual_steps * (SHEAR_STEPS / ANGLE_STEPS) as i32 + 16;
    SHEARS_128[index as usize]
}

fn round_shift(numer: i64) -> i64 {
    round_div_i128(i128::from(numer), i128::from(SHEAR_SCALE)) as i64
}

fn round_div_i128(numer: i128, denominator: i128) -> i128 {
    let half = denominator / 2;
    if numer >= 0 {
        (numer + half) / denominator
    } else {
        (numer - half) / denominator
    }
}

pub(super) fn rotate_vector(step: u32, mut x: i64, mut y: i64) -> (i64, i64) {
    let (quarters, residual_steps) = decompose(step);
    for _ in 0..quarters {
        (x, y) = (-y, x);
    }
    let (t, s) = residual_shears(residual_steps);
    x -= round_shift(t * y);
    y += round_shift(s * x);
    x -= round_shift(t * y);
    (x, y)
}

pub(super) fn rotate_offset(step: u32, dx: i32, dy: i32) -> (i32, i32) {
    let (x, y) = rotate_vector(step, i64::from(dx), i64::from(dy));
    (x as i32, y as i32)
}
