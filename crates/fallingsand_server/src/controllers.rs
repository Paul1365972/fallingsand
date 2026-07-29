use crate::player::{Avatar, PlayerLife, Players};
use crate::species::flesh::{self, DUCK_ROWS, STAND_ROWS};
use fallingsand_core::content;
use fallingsand_core::{CellPos, Phase, Subcell, TICK_DT};
use fallingsand_protocol::GameMode;
use fallingsand_sim::CellWorld;
use fallingsand_sim::body::Bodies;

const MIN_TRACTION: f32 = 0.06;
const COYOTE_SECS: f32 = 0.1;
const BUFFER_SECS: f32 = 0.1;
const VAR_JUMP_SECS: f32 = 0.2;
const POSTURE_STEP_SECS: f32 = 1.0 / 50.0;
const SWIM_CONTROL_MIN_SUBMERSION: f32 = 0.5;
const BANK_VAULT_MIN_SUBMERSION: f32 = 0.2;
const BANK_VAULT_MAX_SUBMERSION: f32 = 0.95;
const BANK_VAULT_MAX_SINK: Subcell = Subcell::from_cells_per_second(20.0);
const BANK_PROBE_CELLS: i32 = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StepInput {
    pub move_x: i8,
    pub jump: bool,
    pub jump_pressed: bool,
    pub down: bool,
    pub fly: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerParams {
    pub max_run: Subcell,
    pub run_accel: Subcell,
    pub run_reduce: Subcell,
    pub air_mult: f32,
    pub duck_friction: Subcell,
    pub duck_run_mult: f32,
    pub gravity_scale: f32,
    pub apex_scale: f32,
    pub apex_threshold: Subcell,
    pub max_fall: Subcell,
    pub fast_max_fall: Subcell,
    pub fast_max_accel: Subcell,
    pub jump_speed: Subcell,
    pub jump_h_boost: Subcell,
    pub swim_speed: Subcell,
    pub swim_thrust: Subcell,
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
            gravity_scale: 1.5,
            apex_scale: 0.5,
            apex_threshold: Subcell::from_cells_per_second(40.0),
            max_fall: Subcell::from_cells_per_second(160.0),
            fast_max_fall: Subcell::from_cells_per_second(240.0),
            fast_max_accel: Subcell::from_cells_per_second_squared(300),
            jump_speed: Subcell::from_cells_per_second(105.0),
            jump_h_boost: Subcell::from_cells_per_second(40.0),
            swim_speed: Subcell::from_cells_per_second(70.0),
            swim_thrust: Subcell::from_cells_per_second_squared(1200),
            wade_run_mult: 0.5,
            fly_max: Subcell::from_cells_per_second(160.0),
            fly_accel: Subcell::from_cells_per_second_squared(1200),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Controller {
    pub rows: i32,
    pub facing_left: bool,
    coyote: f32,
    buffer: f32,
    var_jump_timer: f32,
    var_jump_speed: Subcell,
    max_fall: Subcell,
    posture_step: f32,
}

impl Controller {
    pub fn new(rows: i32) -> Self {
        Self {
            rows,
            facing_left: false,
            coyote: 0.0,
            buffer: 0.0,
            var_jump_timer: 0.0,
            var_jump_speed: Subcell::ZERO,
            max_fall: Subcell::ZERO,
            posture_step: 0.0,
        }
    }
}

fn same_direction(v: Subcell, dir: i32) -> bool {
    (dir > 0 && v > Subcell::ZERO) || (dir < 0 && v < Subcell::ZERO)
}

pub fn drive_player(
    bodies: &mut Bodies,
    sim: &mut CellWorld,
    params: &PlayerParams,
    avatar: &mut Avatar,
    input: StepInput,
) {
    let id = avatar.body_id;
    let Some((mut vx, mut vy)) = bodies.velocity(id) else {
        return;
    };
    let grounded = bodies.supported(sim, id);
    let ceiling = bodies.touching(sim, id, 0, 1);
    let submersion = bodies.submersion(sim, id);
    let weight = bodies.weight(id);
    let move_x = i32::from(input.move_x.clamp(-1, 1));

    let ctrl = &mut avatar.controller;
    ctrl.buffer = if input.jump_pressed {
        BUFFER_SECS
    } else {
        (ctrl.buffer - TICK_DT).max(0.0)
    };
    ctrl.coyote = if grounded {
        COYOTE_SECS
    } else {
        (ctrl.coyote - TICK_DT).max(0.0)
    };
    ctrl.var_jump_timer = if ceiling {
        0.0
    } else {
        (ctrl.var_jump_timer - TICK_DT).max(0.0)
    };
    ctrl.posture_step = (ctrl.posture_step - TICK_DT).max(-POSTURE_STEP_SECS);

    let swimming = !grounded && submersion >= SWIM_CONTROL_MIN_SUBMERSION;
    let ducking = !input.fly && !swimming && input.down;
    let target_rows = if ducking { DUCK_ROWS } else { STAND_ROWS };
    step_posture(bodies, sim, avatar, target_rows);
    face(bodies, sim, avatar, move_x);
    let vaultable = avatar.controller.buffer > 0.0
        && avatar.controller.coyote <= 0.0
        && (BANK_VAULT_MIN_SUBMERSION..=BANK_VAULT_MAX_SUBMERSION).contains(&submersion)
        && vy >= -BANK_VAULT_MAX_SINK
        && bank_ahead(sim, bodies, avatar, move_x);

    let ctrl = &mut avatar.controller;
    if input.fly {
        ctrl.buffer = 0.0;
        ctrl.coyote = 0.0;
        ctrl.var_jump_timer = 0.0;
        let move_y = i32::from(input.jump) - i32::from(input.down);
        vx = vx.approach(params.fly_max.times(move_x), params.fly_accel);
        vy = vy.approach(params.fly_max.times(move_y), params.fly_accel);
        bodies.drive(id, vx, vy);
        return;
    }

    if swimming {
        if move_x != 0 {
            vx = vx.approach(
                params.swim_speed.scaled_by(submersion).times(move_x),
                params.swim_thrust.scaled_by(submersion),
            );
        }
    } else {
        let traction = if grounded {
            bodies
                .support_traction(sim, id)
                .map_or(1.0, |grip| grip.clamp(MIN_TRACTION, 1.0))
        } else {
            1.0
        };
        if grounded && ctrl.rows < STAND_ROWS {
            let target = params.max_run.scaled_by(params.duck_run_mult).times(move_x);
            let rate = if move_x == 0 {
                params.duck_friction
            } else {
                params.run_accel
            };
            vx = vx.approach(target, rate.scaled_by(traction));
        } else {
            let mult = if grounded { traction } else { params.air_mult };
            let wade = 1.0 - (1.0 - params.wade_run_mult) * submersion;
            let max_run = params.max_run.scaled_by(wade);
            let rate = if same_direction(vx, move_x) && vx.abs() > max_run {
                params.run_reduce
            } else {
                params.run_accel
            };
            vx = vx.approach(max_run.times(move_x), rate.scaled_by(mult));
        }
    }

    ctrl.max_fall = ctrl.max_fall.max(params.max_fall);
    let fast = input.down && vy <= -params.max_fall;
    let fall_target = if fast {
        params.fast_max_fall
    } else {
        params.max_fall
    };
    ctrl.max_fall = ctrl.max_fall.approach(fall_target, params.fast_max_accel);

    if !grounded {
        let apex = vy.abs() <= params.apex_threshold && input.jump;
        let hang = if apex { params.apex_scale } else { 1.0 };
        let hang = hang + (1.0 - hang) * submersion;
        vy += weight.scaled_by(params.gravity_scale * hang - 1.0);
        vy = vy.max(-ctrl.max_fall);
        let move_y = i32::from(input.jump) - i32::from(input.down);
        if move_y != 0 && swimming {
            vy = vy.approach(
                params.swim_speed.scaled_by(submersion).times(move_y),
                params.swim_thrust.scaled_by(submersion),
            );
        }
    }

    if ctrl.var_jump_timer > 0.0 {
        if input.jump {
            vy = vy.max(ctrl.var_jump_speed);
        } else {
            ctrl.var_jump_timer = 0.0;
        }
    }

    if ctrl.buffer > 0.0 {
        if ctrl.coyote > 0.0 {
            let footing = (1.0 - submersion).clamp(0.0, 1.0).sqrt();
            jump(params, ctrl, &mut vx, &mut vy, move_x, footing);
        } else if vaultable {
            jump(params, ctrl, &mut vx, &mut vy, move_x, 1.0);
        }
    }
    bodies.drive(id, vx, vy);
}

fn jump(
    params: &PlayerParams,
    ctrl: &mut Controller,
    vx: &mut Subcell,
    vy: &mut Subcell,
    move_x: i32,
    scale: f32,
) {
    ctrl.buffer = 0.0;
    ctrl.coyote = 0.0;
    *vx += params.jump_h_boost.scaled_by(scale).times(move_x);
    *vy = params.jump_speed.scaled_by(scale);
    ctrl.var_jump_timer = VAR_JUMP_SECS;
    ctrl.var_jump_speed = *vy;
}

fn step_posture(bodies: &mut Bodies, sim: &mut CellWorld, avatar: &mut Avatar, target_rows: i32) {
    let rows = avatar.controller.rows;
    if rows == target_rows || avatar.controller.posture_step > 0.0 {
        return;
    }
    let next = if target_rows > rows {
        rows + 1
    } else {
        rows - 1
    };
    let Some(anchor) = bodies.cell(avatar.body_id) else {
        return;
    };
    let cells = flesh::cells(
        flesh::anchor(anchor.x, flesh::feet(anchor, rows), next),
        next,
        avatar.controller.facing_left,
    );
    if bodies.reshape(sim, avatar.body_id, &cells) {
        avatar.controller.rows = next;
        avatar.controller.posture_step += POSTURE_STEP_SECS;
    }
}

fn face(bodies: &mut Bodies, sim: &mut CellWorld, avatar: &mut Avatar, move_x: i32) {
    let facing_left = match move_x {
        x if x < 0 => true,
        x if x > 0 => false,
        _ => avatar.controller.facing_left,
    };
    if facing_left == avatar.controller.facing_left {
        return;
    }
    avatar.controller.facing_left = facing_left;
    let rows = avatar.controller.rows;
    bodies.repaint(sim, avatar.body_id, |dx, dy| {
        flesh::frame(rows).shade(dx, dy, facing_left)
    });
}

fn bank_ahead(sim: &CellWorld, bodies: &Bodies, avatar: &Avatar, move_x: i32) -> bool {
    let Some(anchor) = bodies.cell(avatar.body_id) else {
        return false;
    };
    let rect = flesh::rect(anchor, avatar.controller.rows);
    let dirs: &[i32] = match move_x {
        1 => &[1],
        -1 => &[-1],
        _ => &[-1, 1],
    };
    for &dir in dirs {
        let edge = if dir > 0 { rect.max.x } else { rect.min.x };
        for off in 1..=BANK_PROBE_CELLS {
            for y in rect.rows() {
                let blocked = sim
                    .get_cell(CellPos::new(edge + dir * off, y))
                    .is_none_or(|cell| {
                        matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
                    });
                if blocked {
                    return true;
                }
            }
        }
    }
    false
}

pub fn run(sim: &mut CellWorld, players: &mut Players, bodies: &mut Bodies) {
    let params = PlayerParams::default();
    for (_, player) in players.iter_mut() {
        let input = player.inbox.input;
        let jump_pressed = std::mem::take(&mut player.inbox.jump_pressed);
        let creative = player.profile.mode == GameMode::Creative;
        let PlayerLife::Alive(avatar) = &mut player.life else {
            continue;
        };
        drive_player(
            bodies,
            sim,
            &params,
            avatar,
            StepInput {
                move_x: input.move_x(),
                jump: input.jump,
                jump_pressed,
                down: input.down,
                fly: avatar.flying && creative,
            },
        );
    }
}
