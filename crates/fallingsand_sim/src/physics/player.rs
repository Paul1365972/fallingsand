use super::movement::move_body;
use super::{
    Actor, CellSource, MoveResult, OwnCells, StepInput, Submersion, cell_blocks, fluid_drag,
    ring_submersion,
};
use crate::player::{DUCK_ROWS, STAND_ROWS};
use fallingsand_core::content;
use fallingsand_core::{CellPos, Phase, Subcell, TICK_DT};

const MIN_GRIP: f32 = 0.06;
const COYOTE_SECS: f32 = 0.1;
const POSTURE_STEP_INTERVAL_SECS: f32 = 1.0 / 50.0;
const BUFFER_SECS: f32 = 0.1;
const VAR_JUMP_TIME: f32 = 0.2;
const CEILING_VAR_JUMP_GRACE: f32 = 0.15;
const SWIM_CONTROL_MIN_SUBMERSION: f32 = 0.5;
const BANK_VAULT_MIN_SUBMERSION: f32 = 0.2;
const BANK_VAULT_MAX_SUBMERSION: f32 = 0.95;
const BANK_VAULT_MAX_SINK: Subcell = Subcell::from_cells_per_second(20.0);
const BANK_PROBE_CELLS: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerParams {
    pub max_run: Subcell,
    pub run_accel: Subcell,
    pub run_reduce: Subcell,
    pub air_mult: f32,
    pub duck_friction: Subcell,
    pub duck_run_mult: f32,
    pub gravity: Subcell,
    pub half_grav_threshold: Subcell,
    pub max_fall: Subcell,
    pub fast_max_fall: Subcell,
    pub fast_max_accel: Subcell,
    pub jump_speed: Subcell,
    pub jump_h_boost: Subcell,
    pub swim_thrust: Subcell,
    pub density: f32,
    pub wade_run_mult: f32,
    pub fly_max: Subcell,
    pub fly_accel: Subcell,
}

impl Default for PlayerParams {
    fn default() -> Self {
        Self {
            max_run: Subcell::from_cells_per_second(90.0),
            run_accel: Subcell::from_cells_per_second_squared(1000),
            run_reduce: Subcell::from_cells_per_second_squared(400),
            air_mult: 0.65,
            duck_friction: Subcell::from_cells_per_second_squared(500),
            duck_run_mult: 0.4,
            gravity: Subcell::from_cells_per_second_squared(900),
            half_grav_threshold: Subcell::from_cells_per_second(40.0),
            max_fall: Subcell::from_cells_per_second(160.0),
            fast_max_fall: Subcell::from_cells_per_second(240.0),
            fast_max_accel: Subcell::from_cells_per_second_squared(300),
            jump_speed: Subcell::from_cells_per_second(105.0),
            jump_h_boost: Subcell::from_cells_per_second(40.0),
            swim_thrust: Subcell::from_cells_per_second_squared(450),
            density: 1050.0,
            wade_run_mult: 0.5,
            fly_max: Subcell::from_cells_per_second(160.0),
            fly_accel: Subcell::from_cells_per_second_squared(1200),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Controller {
    coyote: f32,
    buffer: f32,
    var_jump_timer: f32,
    var_jump_speed: Subcell,
    max_fall: Subcell,
    duck_step: f32,
}

fn approach(value: Subcell, target: Subcell, delta: Subcell) -> Subcell {
    if value < target {
        (value + delta).min(target)
    } else {
        (value - delta).max(target)
    }
}

fn same_direction(v: Subcell, dir: i32) -> bool {
    (dir > 0 && v > Subcell::ZERO) || (dir < 0 && v < Subcell::ZERO)
}

fn ground_grip<W: CellSource>(world: &W, body: &Actor) -> f32 {
    let fp = body.footprint();
    let feet = fp.y0 - 1;
    let mut grip = 0.0f32;
    let mut found = false;
    for x in fp.x0..=fp.x1 {
        if let Some(cell) = world.cell_at(CellPos::new(x, feet))
            && matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
        {
            found = true;
            grip = grip.max(content::material(cell.material).surface_grip);
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
    ctrl.duck_step = (ctrl.duck_step - TICK_DT).max(-POSTURE_STEP_INTERVAL_SECS);

    let move_x = input.move_x.clamp(-1, 1) as i32;
    let submersion = ring_submersion(world, body);
    if input.fly {
        fly_update(world, params, body, ctrl, move_x, jump_held, down_held);
    } else {
        normal_update(
            world, params, body, ctrl, move_x, jump_held, down_held, submersion,
        );
    }

    let result = move_body(world, body, submersion.fraction, own);
    if result.corrected_ceiling
        || (result.hit_ceiling && ctrl.var_jump_timer < CEILING_VAR_JUMP_GRACE)
    {
        ctrl.var_jump_timer = 0.0;
    }
    result
}

fn fly_update<W: CellSource>(
    world: &W,
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
    step_height(world, body, ctrl, STAND_ROWS as i32);
    let move_y = jump_held as i32 - down_held as i32;
    body.vx = approach(body.vx, params.fly_max.times(move_x), params.fly_accel);
    body.vy = approach(body.vy, params.fly_max.times(move_y), params.fly_accel);
}

#[allow(clippy::too_many_arguments)]
fn normal_update<W: CellSource>(
    world: &W,
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
    step_height(world, body, ctrl, target_rows);

    let grip = if body.on_ground {
        ground_grip(world, body)
    } else {
        1.0
    };
    if body.on_ground && body.rows() < STAND_ROWS as i32 {
        let target = params.max_run.scaled_by(params.duck_run_mult).times(move_x);
        let rate = if move_x == 0 {
            params.duck_friction
        } else {
            params.run_accel
        };
        body.vx = approach(body.vx, target, rate.scaled_by(grip));
    } else {
        let mult = if body.on_ground {
            grip
        } else {
            params.air_mult
        };
        let wade = 1.0 - (1.0 - params.wade_run_mult) * submersion.fraction;
        let max_run = params.max_run.scaled_by(wade);
        let target = max_run.times(move_x);
        let rate = if same_direction(body.vx, move_x) && body.vx.abs() > max_run {
            params.run_reduce
        } else {
            params.run_accel
        };
        body.vx = approach(body.vx, target, rate.scaled_by(mult));
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
        let net = params.gravity.scaled_by(assist - buoyancy);
        if net >= Subcell::ZERO {
            body.vy = approach(body.vy, -ctrl.max_fall, net);
        } else {
            body.vy -= net;
        }
        let move_y = jump_held as i32 - down_held as i32;
        if move_y != 0 && submersion.fraction > 0.0 {
            let thrust = params
                .swim_thrust
                .scaled_by(submersion.fraction)
                .times(move_y);
            body.vy += thrust;
        }
    }

    if submersion.fraction > 0.0 {
        let vx = body.vx.to_cells_per_second();
        let vy = body.vy.to_cells_per_second();
        let rel_x = vx - submersion.flow_vx;
        let rel_y = vy - submersion.flow_vy;
        let speed = rel_x.hypot(rel_y);
        let drag = fluid_drag(speed, submersion.fraction);
        body.vx = Subcell::from_cells_per_second(vx - rel_x * drag);
        body.vy = Subcell::from_cells_per_second(vy - rel_y * drag);
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
            jump(params, body, ctrl, move_x, weight.sqrt());
        } else if submersion.fraction >= BANK_VAULT_MIN_SUBMERSION
            && submersion.fraction <= BANK_VAULT_MAX_SUBMERSION
            && body.vy >= -BANK_VAULT_MAX_SINK
            && bank_ahead(world, body, move_x)
        {
            jump(params, body, ctrl, move_x, 1.0);
        }
    }
}

fn bank_ahead<W: CellSource>(world: &W, body: &Actor, move_x: i32) -> bool {
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
                if cell_blocks(world, CellPos::new(edge + dir * off, y)) {
                    return true;
                }
            }
        }
    }
    false
}

fn jump(params: &PlayerParams, body: &mut Actor, ctrl: &mut Controller, move_x: i32, scale: f32) {
    ctrl.buffer = 0.0;
    ctrl.coyote = 0.0;
    body.vx += params.jump_h_boost.scaled_by(scale).times(move_x);
    body.vy = params.jump_speed.scaled_by(scale);
    ctrl.var_jump_timer = VAR_JUMP_TIME;
    ctrl.var_jump_speed = body.vy;
}

fn step_height<W: CellSource>(
    world: &W,
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
            if cell_blocks(world, CellPos::new(x, fp.y1 + 1)) {
                return;
            }
        }
    }
    body.y += Subcell::from_cells((next / 2 - rows / 2) as f32);
    body.half_h = Subcell::from_cells(next as f32).scaled_by(0.5);
    ctrl.duck_step += POSTURE_STEP_INTERVAL_SECS;
}
