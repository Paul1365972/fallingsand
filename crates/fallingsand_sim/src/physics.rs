use crate::obstacles::Obstacles;
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, MaterialRegistry, Phase};

pub const STEP_UP_CELLS: i32 = 3;
pub const STEP_DOWN_CELLS: i32 = 3;
const CEILING_SLIP_CELLS: i32 = 2;
const SUB_STEP: f32 = 0.4;
const SKIN: f32 = 1e-4;
const COYOTE_SECS: f32 = 0.13;
const BUFFER_SECS: f32 = 0.13;
const APEX_SPEED: f32 = 20.0;
const STEP_UP_MAX_RISE: f32 = 40.0;
const WADE_UP_CELLS: usize = 4;
const WADE_SIDE_CELLS: usize = 2;
const FALL_RAMP_SPEED: f32 = 110.0;
const FAST_FALL_GRAVITY: f32 = 1.3;
const FALL_CAP_EASE: f32 = 2000.0;
const HOP_BOOST: f32 = 30.0;
const OVERSPEED_FRICTION_GROUND: f32 = 400.0;
const OVERSPEED_FRICTION_AIR: f32 = 60.0;
const WADE_DAMP: f32 = 0.7;
const MAX_DISPLACED: usize = 16;
const SCATTER_RADIUS: i32 = 6;
const SURFACE_BOB_SPEED: f32 = 40.0;
const FLOAT_ON_DEPTH: f32 = 2.0;
const FLOAT_OFF_DEPTH: f32 = 4.0;

pub trait CellSource {
    fn cell_at(&self, pos: CellPos) -> Option<Cell>;
}

impl CellSource for CellWorld {
    fn cell_at(&self, pos: CellPos) -> Option<Cell> {
        self.get_cell(pos)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub half_w: f32,
    pub half_h: f32,
    pub on_ground: bool,
}

impl Body {
    pub fn new(x: f32, y: f32, half_w: f32, half_h: f32) -> Self {
        Self {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            half_w,
            half_h,
            on_ground: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Controller {
    coyote: f32,
    buffer: f32,
    jump_held: bool,
    jumping: bool,
    submerged: bool,
    floating: bool,
}

pub fn cell_blocks<W: CellSource>(world: &W, registry: &MaterialRegistry, pos: CellPos) -> bool {
    match world.cell_at(pos) {
        Some(cell) => matches!(
            registry.get(cell.material).phase,
            Phase::Solid | Phase::Powder
        ),
        None => true,
    }
}

pub fn cell_liquid<W: CellSource>(world: &W, registry: &MaterialRegistry, pos: CellPos) -> bool {
    match world.cell_at(pos) {
        Some(cell) => registry.get(cell.material).phase == Phase::Liquid,
        None => false,
    }
}

fn cell_bounds(cx: f32, cy: f32, half_w: f32, half_h: f32) -> (i32, i32, i32, i32) {
    (
        (cx - half_w + SKIN).floor() as i32,
        (cy - half_h + SKIN).floor() as i32,
        (cx + half_w - SKIN).floor() as i32,
        (cy + half_h - SKIN).floor() as i32,
    )
}

fn rect_blocked<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    cx: f32,
    cy: f32,
    half_w: f32,
    half_h: f32,
) -> bool {
    let (x0, y0, x1, y1) = cell_bounds(cx, cy, half_w, half_h);
    for y in y0..=y1 {
        for x in x0..=x1 {
            if cell_blocks(world, registry, CellPos::new(x, y)) {
                return true;
            }
        }
    }
    false
}

pub fn body_submerged<W: CellSource>(world: &W, registry: &MaterialRegistry, body: &Body) -> bool {
    cell_liquid(
        world,
        registry,
        CellPos::new(body.x.floor() as i32, body.y.floor() as i32),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerParams {
    pub run_speed: f32,
    pub run_accel: f32,
    pub run_decel: f32,
    pub air_control: f32,
    pub jump_speed: f32,
    pub jump_cut: f32,
    pub cut_gravity: f32,
    pub gravity_up: f32,
    pub gravity_down: f32,
    pub apex_gravity: f32,
    pub max_fall_speed: f32,
    pub fast_fall_speed: f32,
    pub swim_speed: f32,
    pub swim_accel: f32,
    pub water_gravity: f32,
    pub water_buoyancy: f32,
    pub water_thrust: f32,
    pub water_drag: f32,
    pub water_drag_quad: f32,
}

impl Default for PlayerParams {
    fn default() -> Self {
        Self {
            run_speed: 80.0,
            run_accel: 750.0,
            run_decel: 1600.0,
            air_control: 0.65,
            jump_speed: 205.0,
            jump_cut: 0.5,
            cut_gravity: 2.0,
            gravity_up: -760.0,
            gravity_down: -850.0,
            apex_gravity: 0.7,
            max_fall_speed: 190.0,
            fast_fall_speed: 340.0,
            swim_speed: 32.0,
            swim_accel: 250.0,
            water_gravity: 500.0,
            water_buoyancy: 400.0,
            water_thrust: 450.0,
            water_drag: 3.0,
            water_drag_quad: 0.08,
        }
    }
}

fn approach(value: f32, target: f32, delta: f32) -> f32 {
    if value < target {
        (value + delta).min(target)
    } else {
        (value - delta).max(target)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn step_player<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    params: &PlayerParams,
    body: &mut Body,
    ctrl: &mut Controller,
    move_x: i8,
    jump_held: bool,
    down_held: bool,
    dt: f32,
) -> Vec<CellPos> {
    let pressed = jump_held && !ctrl.jump_held;
    ctrl.jump_held = jump_held;
    ctrl.buffer = if pressed {
        BUFFER_SECS
    } else {
        (ctrl.buffer - dt).max(0.0)
    };

    let submerged = body_submerged(world, registry, body);
    if submerged {
        ctrl.coyote = if body.on_ground {
            COYOTE_SECS
        } else {
            (ctrl.coyote - dt).max(0.0)
        };
        if body.vy <= 0.0 {
            ctrl.jumping = false;
        }
        if (ctrl.buffer > 0.0 || jump_held) && ctrl.coyote > 0.0 {
            body.vy = params.jump_speed;
            body.vx += move_x.clamp(-1, 1) as f32 * HOP_BOOST;
            ctrl.buffer = 0.0;
            ctrl.coyote = 0.0;
            ctrl.jumping = true;
        }
        let samples = (body.half_h * 2.0).round().max(1.0) as i32;
        let mut wet = 0;
        for k in 0..samples {
            let sy = body.y - body.half_h + 0.5 + k as f32;
            let pos = CellPos::new(body.x.floor() as i32, sy.floor() as i32);
            if cell_liquid(world, registry, pos) {
                wet += 1;
            }
        }
        let submersion = wet as f32 / samples as f32;
        let float_on = CellPos::new(
            body.x.floor() as i32,
            (body.y + body.half_h - FLOAT_ON_DEPTH).floor() as i32,
        );
        let float_off = CellPos::new(
            body.x.floor() as i32,
            (body.y + body.half_h - FLOAT_OFF_DEPTH).floor() as i32,
        );
        if cell_liquid(world, registry, float_on) {
            ctrl.floating = true;
        } else if !cell_liquid(world, registry, float_off) {
            ctrl.floating = false;
        }
        let lift = params.water_buoyancy
            + if jump_held && ctrl.floating {
                params.water_thrust
            } else {
                0.0
            }
            - if down_held { params.water_thrust } else { 0.0 };
        body.vy += (lift * submersion - params.water_gravity) * dt;
        let drag = params.water_drag + params.water_drag_quad * body.vy.abs();
        body.vy *= 1.0 - (drag * dt).min(1.0);
        let target = move_x.clamp(-1, 1) as f32 * params.swim_speed;
        body.vx = approach(body.vx, target, params.swim_accel * dt);
    } else {
        if ctrl.submerged && !ctrl.jumping && body.vy > SURFACE_BOB_SPEED {
            body.vy = SURFACE_BOB_SPEED;
        }
        ctrl.coyote = if body.on_ground {
            COYOTE_SECS
        } else {
            (ctrl.coyote - dt).max(0.0)
        };
        if (ctrl.buffer > 0.0 || jump_held) && ctrl.coyote > 0.0 {
            body.vy = params.jump_speed;
            body.vx += move_x.clamp(-1, 1) as f32 * HOP_BOOST;
            ctrl.buffer = 0.0;
            ctrl.coyote = 0.0;
            ctrl.jumping = true;
        }
        if ctrl.jumping && body.vy > 0.0 && !jump_held {
            body.vy *= params.jump_cut;
            ctrl.jumping = false;
        }
        if body.vy <= 0.0 {
            ctrl.jumping = false;
        }
        let gravity = if down_held && body.vy <= 0.0 {
            params.gravity_down * FAST_FALL_GRAVITY
        } else if !body.on_ground && jump_held && body.vy.abs() < APEX_SPEED {
            params.gravity_up * params.apex_gravity
        } else if body.vy > 0.0 {
            if jump_held {
                params.gravity_up
            } else {
                params.gravity_up * params.cut_gravity
            }
        } else if body.vy > -FALL_RAMP_SPEED {
            let blend = -body.vy / FALL_RAMP_SPEED;
            params.gravity_up * params.apex_gravity * (1.0 - blend) + params.gravity_down * blend
        } else {
            params.gravity_down
        };
        let cap = if down_held {
            params.fast_fall_speed
        } else {
            params.max_fall_speed
        };
        body.vy += gravity * dt;
        if body.vy < -cap {
            body.vy = approach(body.vy, -cap, FALL_CAP_EASE * dt);
        }
        let target = move_x.clamp(-1, 1) as f32 * params.run_speed;
        if target != 0.0 && target * body.vx > 0.0 && body.vx.abs() > target.abs() {
            let friction = if body.on_ground {
                OVERSPEED_FRICTION_GROUND
            } else {
                OVERSPEED_FRICTION_AIR
            };
            body.vx = approach(body.vx, target, friction * dt);
        } else {
            let mut accel = if target == 0.0 || target * body.vx < 0.0 {
                params.run_decel
            } else {
                params.run_accel
            };
            if !body.on_ground {
                accel *= params.air_control;
            }
            body.vx = approach(body.vx, target, accel * dt);
        }
    }
    ctrl.submerged = submerged;
    move_body(world, registry, body, dt)
}

enum Passage {
    Free,
    Solid,
    Powder(Vec<CellPos>),
}

fn passage<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Body,
    cx: f32,
    cy: f32,
    displaced: &[CellPos],
) -> Passage {
    let cur = cell_bounds(body.x, body.y, body.half_w, body.half_h);
    let (x0, y0, x1, y1) = cell_bounds(cx, cy, body.half_w, body.half_h);
    let mut powder: Vec<CellPos> = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let pos = CellPos::new(x, y);
            let Some(cell) = world.cell_at(pos) else {
                return Passage::Solid;
            };
            match registry.get(cell.material).phase {
                Phase::Solid => return Passage::Solid,
                Phase::Powder => {
                    let overlapped = x >= cur.0 && x <= cur.2 && y >= cur.1 && y <= cur.3;
                    if !overlapped && !displaced.contains(&pos) {
                        powder.push(pos);
                    }
                }
                _ => {}
            }
        }
    }
    if powder.is_empty() {
        Passage::Free
    } else {
        Passage::Powder(powder)
    }
}

pub fn move_body<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &mut Body,
    dt: f32,
) -> Vec<CellPos> {
    let mut displaced: Vec<CellPos> = Vec::new();
    let was_grounded = body.on_ground;
    body.on_ground = false;
    let mut remaining_x = body.vx * dt;
    let mut remaining_y = body.vy * dt;
    let mut slip_budget = CEILING_SLIP_CELLS;

    while remaining_x.abs() > SKIN {
        let step = remaining_x.clamp(-SUB_STEP, SUB_STEP);
        remaining_x -= step;
        let next_x = body.x + step;
        let outcome = passage(world, registry, body, next_x, body.y, &displaced);
        if matches!(outcome, Passage::Free) {
            body.x = next_x;
            continue;
        }
        let mut stepped = false;
        if body.vy <= STEP_UP_MAX_RISE {
            for up in 1..=STEP_UP_CELLS {
                let next_y = body.y + up as f32;
                if !rect_blocked(world, registry, next_x, next_y, body.half_w, body.half_h) {
                    body.x = next_x;
                    body.y = next_y;
                    stepped = true;
                    break;
                }
            }
        }
        if stepped {
            continue;
        }
        if let Passage::Powder(cells) = outcome
            && cells.len() <= WADE_SIDE_CELLS
            && displaced.len() + cells.len() <= MAX_DISPLACED
        {
            let damp = WADE_DAMP.powi(cells.len() as i32);
            displaced.extend(cells);
            body.x = next_x;
            body.vx *= damp;
            remaining_x *= damp;
            continue;
        }
        body.vx = 0.0;
        break;
    }

    if was_grounded
        && body.vy <= 0.0
        && !rect_blocked(
            world,
            registry,
            body.x,
            body.y - 0.1,
            body.half_w,
            body.half_h,
        )
    {
        for down in 1..=STEP_DOWN_CELLS {
            let next_y = body.y - down as f32;
            if rect_blocked(world, registry, body.x, next_y, body.half_w, body.half_h) {
                break;
            }
            if rect_blocked(
                world,
                registry,
                body.x,
                next_y - 0.1,
                body.half_w,
                body.half_h,
            ) {
                body.y = next_y;
                body.on_ground = true;
                break;
            }
        }
    }

    while remaining_y.abs() > SKIN {
        let step = remaining_y.clamp(-SUB_STEP, SUB_STEP);
        let next_y = body.y + step;
        let outcome = passage(world, registry, body, body.x, next_y, &displaced);
        match outcome {
            Passage::Free => {
                remaining_y -= step;
                body.y = next_y;
            }
            Passage::Powder(cells)
                if step > 0.0
                    && cells.len() <= WADE_UP_CELLS
                    && displaced.len() + cells.len() <= MAX_DISPLACED =>
            {
                remaining_y -= step;
                body.y = next_y;
                let damp = WADE_DAMP.powi(cells.len() as i32);
                displaced.extend(cells);
                body.vy *= damp;
                remaining_y *= damp;
            }
            _ => {
                if step > 0.0 {
                    let sign = if body.vx >= 0.0 { 1.0 } else { -1.0 };
                    let mut corrected = false;
                    if slip_budget > 0 {
                        for nudge in [sign, -sign] {
                            let nudged_x = body.x + nudge;
                            if !rect_blocked(
                                world,
                                registry,
                                nudged_x,
                                next_y,
                                body.half_w,
                                body.half_h,
                            ) {
                                body.x = nudged_x;
                                body.y = next_y;
                                remaining_y -= step;
                                slip_budget -= 1;
                                corrected = true;
                                break;
                            }
                        }
                    }
                    if !corrected {
                        body.vy = 0.0;
                        break;
                    }
                } else {
                    body.on_ground = true;
                    body.vy = 0.0;
                    break;
                }
            }
        }
    }

    if body.vy <= 0.0
        && !body.on_ground
        && rect_blocked(
            world,
            registry,
            body.x,
            body.y - 0.1,
            body.half_w,
            body.half_h,
        )
    {
        body.on_ground = true;
    }
    displaced
}

pub fn scatter_powder(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    obstacles: &Obstacles,
    body: &Body,
    cells: &[CellPos],
) {
    let dir = if body.vx > 1.0 {
        1
    } else if body.vx < -1.0 {
        -1
    } else {
        0
    };
    for &pos in cells {
        let Some(cell) = world.get_cell(pos) else {
            continue;
        };
        if registry.get(cell.material).phase != Phase::Powder {
            continue;
        }
        world.set_cell(pos, Cell::AIR);
        'search: for radius in 1..=SCATTER_RADIUS {
            let mut ring: Vec<(i32, i32)> = Vec::new();
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs().max(dy.abs()) == radius {
                        ring.push((dx, dy));
                    }
                }
            }
            ring.sort_by_key(|&(dx, dy)| (dir * dx, dy, dx));
            for (dx, dy) in ring {
                let target = pos.translated(dx, dy);
                if obstacles.occupied(target) {
                    continue;
                }
                let (tx, ty) = (target.x as f32 + 0.5, target.y as f32 + 0.5);
                if (tx - body.x).abs() < body.half_w + 0.5
                    && (ty - body.y).abs() < body.half_h + 0.5
                {
                    continue;
                }
                let empty = world
                    .get_cell(target)
                    .is_some_and(|c| registry.get(c.material).phase == Phase::Empty);
                if empty {
                    world.set_cell(target, cell);
                    break 'search;
                }
            }
        }
    }
}
