use super::movement::move_body;
use super::{
    Actor, CellSource, FLUID_DRAG_LINEAR, FLUID_DRAG_QUAD, MAX_FLUID_DRAG, MoveResult, OwnCells,
    StepInput, Submersion, cell_blocks, footprint_at, ring_submersion,
};
use fallingsand_core::{CellPos, Fixed, MaterialRegistry, Phase, TICK_DT};

const MIN_GRIP: f32 = 0.06;
const COYOTE_SECS: f32 = 0.1;
const BUFFER_SECS: f32 = 0.1;
const VAR_JUMP_TIME: f32 = 0.2;
const CEILING_VAR_JUMP_GRACE: f32 = 0.15;
const SWIM_CONTROL_MIN_SUBMERSION: f32 = 0.5;
const BANK_VAULT_MIN_SUBMERSION: f32 = 0.2;
const BANK_VAULT_MAX_SUBMERSION: f32 = 0.95;
const BANK_VAULT_MAX_SINK: Fixed = Fixed::from_int(20);
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
    pub stand_half_h: Fixed,
    pub duck_half_h: Fixed,
    pub swim_thrust: Fixed,
    pub density: f32,
    pub wade_run_mult: Fixed,
    pub fly_max: Fixed,
    pub fly_accel: Fixed,
}

impl Default for PlayerParams {
    fn default() -> Self {
        Self {
            max_run: Fixed::from_int(90),
            run_accel: Fixed::from_int(1000),
            run_reduce: Fixed::from_int(400),
            air_mult: Fixed::from_f32(0.65),
            duck_friction: Fixed::from_int(500),
            duck_run_mult: Fixed::from_f32(0.4),
            gravity: Fixed::from_int(900),
            half_grav_threshold: Fixed::from_int(40),
            max_fall: Fixed::from_int(160),
            fast_max_fall: Fixed::from_int(240),
            fast_max_accel: Fixed::from_int(300),
            jump_speed: Fixed::from_int(105),
            jump_h_boost: Fixed::from_int(40),
            stand_half_h: Fixed::from_f32(crate::player::STAND_ROWS as f32 * 0.5),
            duck_half_h: Fixed::from_f32(crate::player::DUCK_ROWS as f32 * 0.5),
            swim_thrust: Fixed::from_int(600),
            density: 1050.0,
            wade_run_mult: Fixed::from_f32(0.7),
            fly_max: Fixed::from_int(160),
            fly_accel: Fixed::from_int(1200),
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
    ducking: bool,
}

impl Controller {
    pub fn ducking(&self) -> bool {
        self.ducking
    }

    pub fn set_ducking(&mut self, ducking: bool) {
        self.ducking = ducking;
    }
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
    if ctrl.ducking && can_unduck(world, registry, params, body) {
        unduck(params, body, ctrl);
    }
    let move_y = jump_held as i32 - down_held as i32;
    body.vx = approach(
        body.vx,
        params.fly_max.mul_int(move_x),
        params.fly_accel.per_tick(),
    );
    body.vy = approach(
        body.vy,
        params.fly_max.mul_int(move_y),
        params.fly_accel.per_tick(),
    );
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
    if swimming {
        if ctrl.ducking && can_unduck(world, registry, params, body) {
            unduck(params, body, ctrl);
        }
    } else if !ctrl.ducking && down_held {
        duck(params, body, ctrl);
    } else if ctrl.ducking && !down_held && can_unduck(world, registry, params, body) {
        unduck(params, body, ctrl);
    }

    let grip = if body.on_ground {
        Fixed::from_f32(ground_grip(world, registry, body))
    } else {
        Fixed::ONE
    };
    if body.on_ground && ctrl.ducking {
        let target = params.max_run.mul(params.duck_run_mult).mul_int(move_x);
        let rate = if move_x == 0 {
            params.duck_friction
        } else {
            params.run_accel
        };
        body.vx = approach(body.vx, target, rate.mul(grip).per_tick());
    } else {
        let mult = if body.on_ground {
            grip
        } else {
            params.air_mult
        };
        let wade = Fixed::ONE
            - (Fixed::ONE - params.wade_run_mult).mul(Fixed::from_f32(submersion.fraction));
        let max_run = if body.on_ground {
            params.max_run.mul(wade)
        } else {
            params.max_run
        };
        let target = max_run.mul_int(move_x);
        let rate = if same_direction(body.vx, move_x) && body.vx.abs() > max_run {
            params.run_reduce
        } else {
            params.run_accel
        };
        body.vx = approach(body.vx, target, rate.mul(mult).per_tick());
    }

    ctrl.max_fall = ctrl.max_fall.max(params.max_fall);
    let fast = down_held && body.vy <= -params.max_fall;
    let fall_target = if fast {
        params.fast_max_fall
    } else {
        params.max_fall
    };
    ctrl.max_fall = approach(ctrl.max_fall, fall_target, params.fast_max_accel.per_tick());

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
            body.vy = approach(body.vy, -ctrl.max_fall, net.per_tick());
        } else {
            body.vy -= net.per_tick();
        }
        let move_y = jump_held as i32 - down_held as i32;
        if move_y != 0 && submersion.fraction > 0.0 {
            let thrust = params
                .swim_thrust
                .mul(Fixed::from_f32(submersion.fraction))
                .mul_int(move_y);
            body.vy += thrust.per_tick();
        }
    }

    if submersion.fraction > 0.0 {
        let vx = body.vx.to_f32();
        let vy = body.vy.to_f32();
        let rel_x = vx - submersion.flow_vx;
        let rel_y = vy - submersion.flow_vy;
        let speed = rel_x.hypot(rel_y);
        let drag = ((FLUID_DRAG_LINEAR + FLUID_DRAG_QUAD * speed) * submersion.fraction * TICK_DT)
            .min(MAX_FLUID_DRAG);
        body.vx = Fixed::from_f32(vx - rel_x * drag);
        body.vy = Fixed::from_f32(vy - rel_y * drag);
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

fn duck(params: &PlayerParams, body: &mut Actor, ctrl: &mut Controller) {
    body.y -= params.stand_half_h - params.duck_half_h;
    body.half_h = params.duck_half_h;
    ctrl.ducking = true;
}

fn unduck(params: &PlayerParams, body: &mut Actor, ctrl: &mut Controller) {
    body.y += params.stand_half_h - body.half_h;
    body.half_h = params.stand_half_h;
    ctrl.ducking = false;
}

fn can_unduck<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    params: &PlayerParams,
    body: &Actor,
) -> bool {
    let stand_cy = body.y - body.half_h + params.stand_half_h;
    let cur = body.footprint();
    let next = footprint_at(body.x, stand_cy, body.half_w, params.stand_half_h);
    for y in next.y0..=next.y1 {
        for x in next.x0..=next.x1 {
            let pos = CellPos::new(x, y);
            if !cur.contains(pos) && cell_blocks(world, registry, pos) {
                return false;
            }
        }
    }
    true
}
