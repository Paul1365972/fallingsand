use crate::obstacles::Obstacles;
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, MaterialRegistry, Phase};

pub const STEP_UP_CELLS: i32 = 3;
pub const STEP_DOWN_CELLS: i32 = 3;
const SKIN: f32 = 1e-4;
const COYOTE_SECS: f32 = 0.13;
const BUFFER_SECS: f32 = 0.13;
const APEX_SPEED: f32 = 20.0;
const WADE_UP_CELLS: usize = 4;
const WADE_SIDE_CELLS: usize = 2;
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
    body: &Body,
    cx: f32,
    cy: f32,
) -> bool {
    let cur = cell_bounds(body.x, body.y, body.half_w, body.half_h);
    let (x0, y0, x1, y1) = cell_bounds(cx, cy, body.half_w, body.half_h);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let overlapped = x >= cur.0 && x <= cur.2 && y >= cur.1 && y <= cur.3;
            if !overlapped && cell_blocks(world, registry, CellPos::new(x, y)) {
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

fn bank_adjacent<W: CellSource>(world: &W, registry: &MaterialRegistry, body: &Body) -> bool {
    let left = (body.x - body.half_w - 0.5).floor() as i32;
    let right = (body.x + body.half_w + 0.5).floor() as i32;
    let y0 = body.y.floor() as i32;
    let y1 = (body.y + body.half_h + 1.0).floor() as i32;
    for y in y0..=y1 {
        if cell_blocks(world, registry, CellPos::new(left, y))
            || cell_blocks(world, registry, CellPos::new(right, y))
        {
            return true;
        }
    }
    false
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
    pub fast_fall_gravity: f32,
    pub fall_ramp_speed: f32,
    pub fall_cap_ease: f32,
    pub hop_boost: f32,
    pub overspeed_friction_ground: f32,
    pub overspeed_friction_air: f32,
    pub swim_speed: f32,
    pub swim_accel: f32,
    pub water_gravity: f32,
    pub water_buoyancy: f32,
    pub water_thrust: f32,
    pub water_drag: f32,
    pub water_drag_quad: f32,
    pub water_exit_boost: f32,
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
            cut_gravity: 1.5,
            gravity_up: -760.0,
            gravity_down: -850.0,
            apex_gravity: 0.7,
            max_fall_speed: 190.0,
            fast_fall_speed: 340.0,
            fast_fall_gravity: 1.3,
            fall_ramp_speed: 110.0,
            fall_cap_ease: 2000.0,
            hop_boost: 30.0,
            overspeed_friction_ground: 400.0,
            overspeed_friction_air: 60.0,
            swim_speed: 32.0,
            swim_accel: 250.0,
            water_gravity: 500.0,
            water_buoyancy: 400.0,
            water_thrust: 450.0,
            water_drag: 3.0,
            water_drag_quad: 0.08,
            water_exit_boost: 1.25,
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
) -> MoveResult {
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
        let bank = bank_adjacent(world, registry, body);
        if (ctrl.buffer > 0.0 || jump_held) && (ctrl.coyote > 0.0 || (ctrl.floating && bank)) {
            let boost = if bank { params.water_exit_boost } else { 1.0 };
            body.vy = params.jump_speed * boost;
            body.vx += move_x.clamp(-1, 1) as f32 * params.hop_boost;
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
            body.vx += move_x.clamp(-1, 1) as f32 * params.hop_boost;
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
            params.gravity_down * params.fast_fall_gravity
        } else if !body.on_ground && body.vy.abs() < APEX_SPEED {
            params.gravity_up * params.apex_gravity
        } else if body.vy > 0.0 {
            if jump_held {
                params.gravity_up
            } else {
                params.gravity_up * params.cut_gravity
            }
        } else if body.vy > -params.fall_ramp_speed {
            let blend = -body.vy / params.fall_ramp_speed;
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
            body.vy = approach(body.vy, -cap, params.fall_cap_ease * dt);
        }
        let target = move_x.clamp(-1, 1) as f32 * params.run_speed;
        if target != 0.0 && target * body.vx > 0.0 && body.vx.abs() > target.abs() {
            let friction = if body.on_ground {
                params.overspeed_friction_ground
            } else {
                params.overspeed_friction_air
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blocked {
    pub pos: CellPos,
    pub dvx: f32,
    pub dvy: f32,
}

#[derive(Debug, Default)]
pub struct MoveResult {
    pub displaced: Vec<CellPos>,
    pub blocked: Vec<Blocked>,
}

impl MoveResult {
    fn record_blocked(&mut self, solids: &[CellPos], dvx: f32, dvy: f32) {
        if solids.is_empty() {
            return;
        }
        let share = 1.0 / solids.len() as f32;
        for &pos in solids {
            self.blocked.push(Blocked {
                pos,
                dvx: dvx * share,
                dvy: dvy * share,
            });
        }
    }
}

struct Blockage {
    solid: bool,
    solids: Vec<CellPos>,
    powder: Vec<CellPos>,
}

impl Blockage {
    fn free(&self) -> bool {
        !self.solid && self.powder.is_empty()
    }

    fn wadeable(&self, limit: usize, displaced: &[CellPos]) -> bool {
        !self.solid
            && !self.powder.is_empty()
            && self.powder.len() <= limit
            && displaced.len() + self.powder.len() <= MAX_DISPLACED
    }

    fn wade(self, displaced: &mut Vec<CellPos>) -> f32 {
        let damp = WADE_DAMP.powi(self.powder.len() as i32);
        displaced.extend(self.powder);
        damp
    }

    fn dig(self, displaced: &mut Vec<CellPos>) {
        let budget = MAX_DISPLACED.saturating_sub(displaced.len());
        displaced.extend(self.powder.into_iter().take(budget));
    }
}

fn passage<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Body,
    cx: f32,
    cy: f32,
    displaced: &[CellPos],
) -> Blockage {
    let cur = cell_bounds(body.x, body.y, body.half_w, body.half_h);
    let (x0, y0, x1, y1) = cell_bounds(cx, cy, body.half_w, body.half_h);
    let mut blockage = Blockage {
        solid: false,
        solids: Vec::new(),
        powder: Vec::new(),
    };
    for y in y0..=y1 {
        for x in x0..=x1 {
            let pos = CellPos::new(x, y);
            let overlapped = x >= cur.0 && x <= cur.2 && y >= cur.1 && y <= cur.3;
            let Some(cell) = world.cell_at(pos) else {
                blockage.solid = true;
                continue;
            };
            if overlapped {
                continue;
            }
            match registry.get(cell.material).phase {
                Phase::Solid => {
                    blockage.solid = true;
                    blockage.solids.push(pos);
                }
                Phase::Powder if !displaced.contains(&pos) => blockage.powder.push(pos),
                _ => {}
            }
        }
    }
    blockage
}

pub fn move_body<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &mut Body,
    dt: f32,
) -> MoveResult {
    let mut result = MoveResult::default();
    let was_grounded = body.on_ground;
    body.on_ground = false;
    let remaining_x = body.vx * dt;
    let remaining_y = body.vy * dt;

    if remaining_x.abs() > SKIN {
        let dir = if remaining_x > 0.0 { 1i32 } else { -1 };
        let dirf = dir as f32;
        let mut target = body.x + remaining_x;
        let mut col = (body.x + dirf * body.half_w - dirf * SKIN).floor() as i32;
        loop {
            let next_col = col + dir;
            let entry_face = if dir > 0 {
                next_col as f32 + SKIN
            } else {
                (next_col + 1) as f32 - 2.0 * SKIN
            };
            let next_x = entry_face - dirf * body.half_w;
            if (next_x - target) * dirf >= 0.0 {
                let blockage = passage(world, registry, body, target, body.y, &result.displaced);
                if blockage.free() {
                    body.x = target;
                } else {
                    result.record_blocked(&blockage.solids, body.vx, 0.0);
                    body.vx = 0.0;
                }
                break;
            }
            let blockage = passage(world, registry, body, next_x, body.y, &result.displaced);
            if blockage.free() {
                body.x = next_x;
                col = next_col;
                continue;
            }
            let mut stepped = false;
            for up in 1..=STEP_UP_CELLS {
                let next_y = body.y + up as f32;
                if !rect_blocked(world, registry, body, next_x, next_y) {
                    body.x = next_x;
                    body.y = next_y;
                    col = next_col;
                    stepped = true;
                    break;
                }
            }
            if stepped {
                continue;
            }
            if blockage.wadeable(WADE_SIDE_CELLS, &result.displaced) {
                let damp = blockage.wade(&mut result.displaced);
                body.x = next_x;
                col = next_col;
                body.vx *= damp;
                target = body.x + (target - body.x) * damp;
                continue;
            }
            result.record_blocked(&blockage.solids, body.vx, 0.0);
            blockage.dig(&mut result.displaced);
            body.x = if dir > 0 {
                next_col as f32 - body.half_w
            } else {
                (next_col + 1) as f32 + body.half_w
            };
            body.vx = 0.0;
            break;
        }
    }

    if was_grounded && body.vy <= 0.0 && !rect_blocked(world, registry, body, body.x, body.y - 0.1)
    {
        for down in 1..=STEP_DOWN_CELLS {
            let next_y = body.y - down as f32;
            if rect_blocked(world, registry, body, body.x, next_y) {
                break;
            }
            if rect_blocked(world, registry, body, body.x, next_y - 0.1) {
                body.y = next_y;
                body.on_ground = true;
                break;
            }
        }
    }

    if remaining_y.abs() > SKIN {
        let dir = if remaining_y > 0.0 { 1i32 } else { -1 };
        let dirf = dir as f32;
        let mut target = body.y + remaining_y;
        let mut row = (body.y + dirf * body.half_h - dirf * SKIN).floor() as i32;
        loop {
            let next_row = row + dir;
            let entry_face = if dir > 0 {
                next_row as f32 + SKIN
            } else {
                (next_row + 1) as f32 - 2.0 * SKIN
            };
            let next_y = entry_face - dirf * body.half_h;
            if (next_y - target) * dirf >= 0.0 {
                let blockage = passage(world, registry, body, body.x, target, &result.displaced);
                if blockage.free() {
                    body.y = target;
                } else {
                    result.record_blocked(&blockage.solids, 0.0, body.vy);
                    body.vy = 0.0;
                }
                break;
            }
            let blockage = passage(world, registry, body, body.x, next_y, &result.displaced);
            if blockage.free() {
                body.y = next_y;
                row = next_row;
            } else if dir > 0 && blockage.wadeable(WADE_UP_CELLS, &result.displaced) {
                let damp = blockage.wade(&mut result.displaced);
                body.y = next_y;
                row = next_row;
                body.vy *= damp;
                target = body.y + (target - body.y) * damp;
            } else {
                result.record_blocked(&blockage.solids, 0.0, body.vy);
                body.y = if dir > 0 {
                    next_row as f32 - body.half_h
                } else {
                    (next_row + 1) as f32 + body.half_h
                };
                if dir < 0 {
                    body.on_ground = true;
                }
                body.vy = 0.0;
                break;
            }
        }
    }

    if body.vy <= 0.0
        && !body.on_ground
        && rect_blocked(world, registry, body, body.x, body.y - 0.1)
    {
        body.on_ground = true;
    }
    result
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
        let mut destination: Option<CellPos> = None;
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
                    destination = Some(target);
                    break 'search;
                }
            }
        }
        if let Some(target) = destination {
            world.set_cell(pos, Cell::AIR);
            world.set_cell(target, cell);
        } else {
            world.mark_keep(pos);
        }
    }
}
