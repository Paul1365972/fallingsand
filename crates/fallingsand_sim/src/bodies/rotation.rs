const SHEAR_BITS: u32 = 16;
const SHEAR_SCALE: i64 = 1 << SHEAR_BITS;

pub(super) const ANGLE_STEPS: u32 = 64;
pub(super) const ANGLE_STEPS_LARGE: u32 = 128;
pub(super) const LARGE_BODY_EXTENT: i32 = 24;

pub(super) fn quantize_step(angle: f32, steps: u32) -> u32 {
    let step = angle / std::f32::consts::TAU * steps as f32;
    (step.round() as i64).rem_euclid(steps as i64) as u32
}

fn decompose(step: u32, steps: u32) -> (u32, i32) {
    let quarter = steps as i32 / 4;
    let quarters = ((step as i64 * 4 + steps as i64 / 2).div_euclid(steps as i64)) as i32;
    let residual_steps = step as i32 - quarters * quarter;
    (quarters.rem_euclid(4) as u32, residual_steps)
}

fn residual_shears(residual_steps: i32, steps: u32) -> (i64, i64) {
    let residual = residual_steps as f32 / steps as f32 * std::f32::consts::TAU;
    let t = fixed(f32::tan(residual / 2.0));
    let s = fixed(f32::sin(residual));
    (t, s)
}

fn fixed(v: f32) -> i64 {
    (v * SHEAR_SCALE as f32).round() as i64
}

fn round_shift(numer: i64) -> i64 {
    let half = SHEAR_SCALE / 2;
    if numer >= 0 {
        (numer + half) / SHEAR_SCALE
    } else {
        (numer - half) / SHEAR_SCALE
    }
}

pub(super) fn rotate_offset(step: u32, steps: u32, dx: i32, dy: i32) -> (i32, i32) {
    let (quarters, residual_steps) = decompose(step, steps);
    let (mut x, mut y) = (dx as i64, dy as i64);
    for _ in 0..quarters {
        let (nx, ny) = (-y, x);
        x = nx;
        y = ny;
    }
    let (t, s) = residual_shears(residual_steps, steps);
    x -= round_shift(t * y);
    y += round_shift(s * x);
    x -= round_shift(t * y);
    (x as i32, y as i32)
}

pub(super) fn unrotate_offset(step: u32, steps: u32, dx: i32, dy: i32) -> (i32, i32) {
    let (quarters, residual_steps) = decompose(step, steps);
    let (t, s) = residual_shears(residual_steps, steps);
    let (mut x, mut y) = (dx as i64, dy as i64);
    x += round_shift(t * y);
    y -= round_shift(s * x);
    x += round_shift(t * y);
    for _ in 0..quarters {
        let (nx, ny) = (y, -x);
        x = nx;
        y = ny;
    }
    (x as i32, y as i32)
}
