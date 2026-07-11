use super::movement::move_body;
use super::{
    Actor, CellSource, FLUID_DRAG_LINEAR, FLUID_DRAG_QUAD, MAX_FLUID_DRAG, MoveResult, OwnCells,
    StepInput, Submersion, cell_blocks, ring_submersion,
};
use crate::player::{DUCK_ROWS, STAND_ROWS};
use fallingsand_core::{CellPos, Fixed, MaterialRegistry, Phase, TICK_DT};

const MIN_GRIP: f32 = 0.06;
const COYOTE_SECS: f32 = 0.1;
const DUCK_STEP_SECS: f32 = 0.016;
const BUFFER_SECS: f32 = 0.1;
const VAR_JUMP_TIME: f32 = 0.2;
const CEILING_VAR_JUMP_GRACE: f32 = 0.15;
const SWIM_CONTROL_MIN_SUBMERSION: f32 = 0.5;
const BANK_VAULT_MIN_SUBMERSION: f32 = 0.2;
const BANK_VAULT_MAX_SUBMERSION: f32 = 0.95;
const BANK_VAULT_MAX_SINK: Fixed = Fixed::vel_per_sec(20.0);
const BANK_PROBE_CELLS: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerParams {
    pub max_run: Fixed,
    pub run_accel: Fixed,
    pub run_reduce: Fixed,
    pub air_mult: Fixed,
    pub duck_friction: Fixed,
    pub duck_run_mult: Fixed,
    pub gravity: Fixed,
    pub half_grav_threshold: Fixed,
    pub max_fall: Fixed,
    pub fast_max_fall: Fixed,
    pub fast_max_accel: Fixed,
    pub jump_speed: Fixed,
    pub jump_h_boost: Fixed,
    pub swim_thrust: Fixed,
    pub density: f32,
    pub wade_run_mult: Fixed,
    pub fly_max: Fixed,
    pub fly_accel: Fixed,
}

impl Default for PlayerParams {
    fn default() -> Self {
        Self {
            max_run: Fixed::vel_per_sec(90.0),
            run_accel: Fixed::accel_per_sec2(1000.0),
            run_reduce: Fixed::accel_per_sec2(400.0),
            air_mult: Fixed::from_f32(0.65),
            duck_friction: Fixed::accel_per_sec2(500.0),
            duck_run_mult: Fixed::from_f32(0.4),
            gravity: Fixed::accel_per_sec2(900.0),
            half_grav_threshold: Fixed::vel_per_sec(40.0),
            max_fall: Fixed::vel_per_sec(160.0),
            fast_max_fall: Fixed::vel_per_sec(240.0),
            fast_max_accel: Fixed::accel_per_sec2(300.0),
            jump_speed: Fixed::vel_per_sec(105.0),
            jump_h_boost: Fixed::vel_per_sec(40.0),
            swim_thrust: Fixed::accel_per_sec2(450.0),
            density: 1050.0,
            wade_run_mult: Fixed::from_f32(0.5),
            fly_max: Fixed::vel_per_sec(160.0),
            fly_accel: Fixed::accel_per_sec2(1200.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Controller {
    coyote: f32,
    buffer: f32,
    var_jump_timer: f32,
    var_jump_speed: Fixed,
    max_fall: Fixed,
    duck_step: f32,
}

fn approach(value: Fixed, target: Fixed, delta: Fixed) -> Fixed {
    if value < target {
        (value + delta).min(target)
    } else {
        (value - delta).max(target)
    }
}

fn same_direction(v: Fixed, dir: i32) -> bool {
    (dir > 0 && v > Fixed::ZERO) || (dir < 0 && v < Fixed::ZERO)
}

fn ground_grip<W: CellSource>(world: &W, registry: &MaterialRegistry, body: &Actor) -> f32 {
    let fp = body.footprint();
    let feet = fp.y0 - 1;
    let mut grip = 0.0f32;
    let mut found = false;
    for x in fp.x0..=fp.x1 {
        if let Some(cell) = world.cell_at(CellPos::new(x, feet)) {
            let material = registry.get(cell.material);
            if matches!(material.phase, Phase::Solid | Phase::Powder) {
                found = true;
                grip = grip.max(material.surface_grip);
            }
        }
    }
    if found {
        grip.clamp(MIN_GRIP, 1.0)
    } else {
        1.0
    }
}

pub fn step_player<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    params: &PlayerParams,
    body: &mut Actor,
    ctrl: &mut Controller,
    input: StepInput,
    own: OwnCells,
) -> MoveResult {
    let jump_held = input.jump;
    let down_held = input.down;
    ctrl.buffer = if input.jump_pressed {
        BUFFER_SECS
    } else {
        (ctrl.buffer - TICK_DT).max(0.0)
    };
    ctrl.coyote = if body.on_ground {
        COYOTE_SECS
    } else {
        (ctrl.coyote - TICK_DT).max(0.0)
    };
    ctrl.var_jump_timer = (ctrl.var_jump_timer - TICK_DT).max(0.0);
    ctrl.duck_step = (ctrl.duck_step - TICK_DT).max(0.0);

    let move_x = input.move_x.clamp(-1, 1) as i32;
    let submersion = ring_submersion(world, registry, body);
    if input.fly {
        fly_update(
            world, registry, params, body, ctrl, move_x, jump_held, down_held,
        );
    } else {
        normal_update(
            world, registry, params, body, ctrl, move_x, jump_held, down_held, submersion,
        );
    }

    let result = move_body(world, registry, body, submersion.fraction, own);
    if result.hit_ceiling && ctrl.var_jump_timer < CEILING_VAR_JUMP_GRACE {
        ctrl.var_jump_timer = 0.0;
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn fly_update<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    params: &PlayerParams,
    body: &mut Actor,
    ctrl: &mut Controller,
    move_x: i32,
    jump_held: bool,
    down_held: bool,
) {
    ctrl.buffer = 0.0;
    ctrl.coyote = 0.0;
    ctrl.var_jump_timer = 0.0;
    step_height(world, registry, body, ctrl, STAND_ROWS as i32);
    let move_y = jump_held as i32 - down_held as i32;
    body.vx = approach(body.vx, params.fly_max.mul_int(move_x), params.fly_accel);
    body.vy = approach(body.vy, params.fly_max.mul_int(move_y), params.fly_accel);
}

#[allow(clippy::too_many_arguments)]
fn normal_update<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    params: &PlayerParams,
    body: &mut Actor,
    ctrl: &mut Controller,
    move_x: i32,
    jump_held: bool,
    down_held: bool,
    submersion: Submersion,
) {
    let swimming = !body.on_ground && submersion.fraction >= SWIM_CONTROL_MIN_SUBMERSION;
    let target_rows = if !swimming && down_held {
        DUCK_ROWS as i32
    } else {
        STAND_ROWS as i32
    };
    step_height(world, registry, body, ctrl, target_rows);

    let grip = if body.on_ground {
        Fixed::from_f32(ground_grip(world, registry, body))
    } else {
        Fixed::ONE
    };
    if body.on_ground && body.rows() < STAND_ROWS as i32 {
        let target = params.max_run.mul(params.duck_run_mult).mul_int(move_x);
        let rate = if move_x == 0 {
            params.duck_friction
        } else {
            params.run_accel
        };
        body.vx = approach(body.vx, target, rate.mul(grip));
    } else {
        let mult = if body.on_ground {
            grip
        } else {
            params.air_mult
        };
        let wade = Fixed::ONE
            - (Fixed::ONE - params.wade_run_mult).mul(Fixed::from_f32(submersion.fraction));
        let max_run = params.max_run.mul(wade);
        let target = max_run.mul_int(move_x);
        let rate = if same_direction(body.vx, move_x) && body.vx.abs() > max_run {
            params.run_reduce
        } else {
            params.run_accel
        };
        body.vx = approach(body.vx, target, rate.mul(mult));
    }

    ctrl.max_fall = ctrl.max_fall.max(params.max_fall);
    let fast = down_held && body.vy <= -params.max_fall;
    let fall_target = if fast {
        params.fast_max_fall
    } else {
        params.max_fall
    };
    ctrl.max_fall = approach(ctrl.max_fall, fall_target, params.fast_max_accel);

    let buoyancy = submersion.fraction * submersion.liquid_density / params.density;
    if !body.on_ground {
        let assist = if body.vy.abs() <= params.half_grav_threshold && jump_held {
            0.5
        } else {
            1.0
        };
        let assist = assist + (1.0 - assist) * submersion.fraction;
        let net = params.gravity.mul(Fixed::from_f32(assist - buoyancy));
        if net >= Fixed::ZERO {
            body.vy = approach(body.vy, -ctrl.max_fall, net);
        } else {
            body.vy -= net;
        }
        let move_y = jump_held as i32 - down_held as i32;
        if move_y != 0 && submersion.fraction > 0.0 {
            let thrust = params
                .swim_thrust
                .mul(Fixed::from_f32(submersion.fraction))
                .mul_int(move_y);
            body.vy += thrust;
        }
    }

    if submersion.fraction > 0.0 {
        let vx = body.vx.vel_f32();
        let vy = body.vy.vel_f32();
        let rel_x = vx - submersion.flow_vx;
        let rel_y = vy - submersion.flow_vy;
        let speed = rel_x.hypot(rel_y);
        let drag = ((FLUID_DRAG_LINEAR + FLUID_DRAG_QUAD * speed) * submersion.fraction * TICK_DT)
            .min(MAX_FLUID_DRAG);
        body.vx = Fixed::vel_per_sec(vx - rel_x * drag);
        body.vy = Fixed::vel_per_sec(vy - rel_y * drag);
    }

    if ctrl.var_jump_timer > 0.0 {
        if jump_held {
            body.vy = body.vy.max(ctrl.var_jump_speed);
        } else {
            ctrl.var_jump_timer = 0.0;
        }
    }

    if ctrl.buffer > 0.0 {
        if ctrl.coyote > 0.0 {
            let weight = (1.0 - buoyancy).clamp(0.0, 1.0);
            jump(params, body, ctrl, move_x, Fixed::from_f32(weight.sqrt()));
        } else if submersion.fraction >= BANK_VAULT_MIN_SUBMERSION
            && submersion.fraction <= BANK_VAULT_MAX_SUBMERSION
            && body.vy >= -BANK_VAULT_MAX_SINK
            && bank_ahead(world, registry, body, move_x)
        {
            jump(params, body, ctrl, move_x, Fixed::ONE);
        }
    }
}

fn bank_ahead<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
    move_x: i32,
) -> bool {
    let fp = body.footprint();
    let dirs: &[i32] = match move_x {
        1 => &[1],
        -1 => &[-1],
        _ => &[-1, 1],
    };
    for &dir in dirs {
        let edge = if dir > 0 { fp.x1 } else { fp.x0 };
        for off in 1..=BANK_PROBE_CELLS {
            for y in fp.y0..=fp.y1 {
                if cell_blocks(world, registry, CellPos::new(edge + dir * off, y)) {
                    return true;
                }
            }
        }
    }
    false
}

fn jump(params: &PlayerParams, body: &mut Actor, ctrl: &mut Controller, move_x: i32, scale: Fixed) {
    ctrl.buffer = 0.0;
    ctrl.coyote = 0.0;
    body.vx += params.jump_h_boost.mul(scale).mul_int(move_x);
    body.vy = params.jump_speed.mul(scale);
    ctrl.var_jump_timer = VAR_JUMP_TIME;
    ctrl.var_jump_speed = body.vy;
}

fn step_height<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &mut Actor,
    ctrl: &mut Controller,
    target_rows: i32,
) {
    let rows = body.rows();
    if rows == target_rows || ctrl.duck_step > 0.0 {
        return;
    }
    let next = if target_rows > rows {
        rows + 1
    } else {
        rows - 1
    };
    if next > rows {
        let fp = body.footprint();
        for x in fp.x0..=fp.x1 {
            if cell_blocks(world, registry, CellPos::new(x, fp.y1 + 1)) {
                return;
            }
        }
    }
    body.y += Fixed::from_int(next / 2 - rows / 2);
    body.half_h = Fixed::from_int(next).mul(Fixed::HALF);
    ctrl.duck_step = DUCK_STEP_SECS;
}
