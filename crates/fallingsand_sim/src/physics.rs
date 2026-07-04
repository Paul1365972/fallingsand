use crate::obstacles::Obstacles;
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, MaterialRegistry, Phase};

pub const STEP_UP_CELLS: i32 = 3;
pub const STEP_DOWN_CELLS: i32 = 3;
const SUB_STEP: f32 = 0.4;
const SKIN: f32 = 1e-4;
const COYOTE_TICKS: u8 = 8;
const BUFFER_TICKS: u8 = 8;
const APEX_SPEED: f32 = 20.0;
const STEP_UP_MAX_RISE: f32 = 40.0;
const WADE_UP_CELLS: usize = 4;
const WADE_SIDE_CELLS: usize = 2;
const FALL_RAMP_SPEED: f32 = 80.0;
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
    coyote: u8,
    buffer: u8,
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
            run_accel: 1000.0,
            run_decel: 1600.0,
            air_control: 0.65,
            jump_speed: 205.0,
            jump_cut: 0.5,
            cut_gravity: 2.0,
            gravity_up: -760.0,
            gravity_down: -1020.0,
            apex_gravity: 0.7,
            max_fall_speed: 330.0,
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
    dt: f32,
) -> Vec<CellPos> {
    let pressed = jump_held && !ctrl.jump_held;
    ctrl.jump_held = jump_held;
    ctrl.buffer = if pressed {
        BUFFER_TICKS
    } else {
        ctrl.buffer.saturating_sub(1)
    };

    let submerged = body_submerged(world, registry, body);
    if submerged {
        ctrl.coyote = if body.on_ground {
            COYOTE_TICKS
        } else {
            ctrl.coyote.saturating_sub(1)
        };
        if body.vy <= 0.0 {
            ctrl.jumping = false;
        }
        if ctrl.buffer > 0 && ctrl.coyote > 0 {
            body.vy = params.jump_speed;
            ctrl.buffer = 0;
            ctrl.coyote = 0;
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
            };
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
            COYOTE_TICKS
        } else {
            ctrl.coyote.saturating_sub(1)
        };
        if ctrl.buffer > 0 && ctrl.coyote > 0 {
            body.vy = params.jump_speed;
            ctrl.buffer = 0;
            ctrl.coyote = 0;
            ctrl.jumping = true;
        }
        if ctrl.jumping && body.vy > 0.0 && !jump_held {
            body.vy *= params.jump_cut;
            ctrl.jumping = false;
        }
        if body.vy <= 0.0 {
            ctrl.jumping = false;
        }
        let gravity = if !body.on_ground && jump_held && body.vy.abs() < APEX_SPEED {
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
        body.vy = (body.vy + gravity * dt).max(-params.max_fall_speed);
        let target = move_x.clamp(-1, 1) as f32 * params.run_speed;
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
    ctrl.submerged = submerged;
    move_body(world, registry, body, dt)
}

enum Passage {
    Free,
    Blocked,
    Wade(u32),
}

fn passage<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Body,
    cx: f32,
    cy: f32,
    wade_limit: usize,
    displaced: &mut Vec<CellPos>,
) -> Passage {
    let cur = cell_bounds(body.x, body.y, body.half_w, body.half_h);
    let (x0, y0, x1, y1) = cell_bounds(cx, cy, body.half_w, body.half_h);
    let mut powder: Vec<CellPos> = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let pos = CellPos::new(x, y);
            let Some(cell) = world.cell_at(pos) else {
                return Passage::Blocked;
            };
            match registry.get(cell.material).phase {
                Phase::Solid => return Passage::Blocked,
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
        return Passage::Free;
    }
    if powder.len() <= wade_limit && displaced.len() + powder.len() <= MAX_DISPLACED {
        let cost = powder.len() as u32;
        displaced.extend(powder);
        Passage::Wade(cost)
    } else {
        Passage::Blocked
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

    while remaining_x.abs() > SKIN {
        let step = remaining_x.clamp(-SUB_STEP, SUB_STEP);
        remaining_x -= step;
        let next_x = body.x + step;
        match passage(
            world,
            registry,
            body,
            next_x,
            body.y,
            WADE_SIDE_CELLS,
            &mut displaced,
        ) {
            Passage::Free => body.x = next_x,
            Passage::Wade(cost) => {
                body.x = next_x;
                let damp = WADE_DAMP.powi(cost as i32);
                body.vx *= damp;
                remaining_x *= damp;
            }
            Passage::Blocked => {
                let mut stepped = false;
                if body.vy <= STEP_UP_MAX_RISE {
                    for up in 1..=STEP_UP_CELLS {
                        let next_y = body.y + up as f32;
                        if !rect_blocked(world, registry, next_x, next_y, body.half_w, body.half_h)
                        {
                            body.x = next_x;
                            body.y = next_y;
                            stepped = true;
                            break;
                        }
                    }
                }
                if !stepped {
                    body.vx = 0.0;
                    break;
                }
            }
        }
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
        match passage(
            world,
            registry,
            body,
            body.x,
            next_y,
            if step > 0.0 { WADE_UP_CELLS } else { 0 },
            &mut displaced,
        ) {
            Passage::Free => {
                remaining_y -= step;
                body.y = next_y;
            }
            Passage::Wade(cost) => {
                remaining_y -= step;
                body.y = next_y;
                let damp = WADE_DAMP.powi(cost as i32);
                body.vy *= damp;
                remaining_y *= damp;
            }
            Passage::Blocked => {
                if step > 0.0 {
                    let nudges: [i32; 4] = if body.vx >= 0.0 {
                        [1, 2, -1, -2]
                    } else {
                        [-1, -2, 1, 2]
                    };
                    let mut corrected = false;
                    for nudge in nudges {
                        let nudged_x = body.x + nudge as f32;
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
                            corrected = true;
                            break;
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
