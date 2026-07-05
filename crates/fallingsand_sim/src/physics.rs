use crate::obstacles::Obstacles;
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, MaterialRegistry, Phase};

pub const STEP_UP_CELLS: i32 = 3;
pub const STEP_DOWN_CELLS: i32 = 3;
const SKIN: f32 = 1e-4;
const COYOTE_SECS: f32 = 0.1;
const BUFFER_SECS: f32 = 0.1;
const VAR_JUMP_TIME: f32 = 0.2;
const CEILING_VAR_JUMP_GRACE: f32 = 0.15;
const UPWARD_CORNER_CORRECTION: i32 = 4;
const SWIM_BEGIN_DAMP: f32 = 0.5;
const UNDERWATER_PROBE_ABOVE_FEET: f32 = 9.0;
const WADE_UP_CELLS: usize = 4;
const WADE_SIDE_CELLS: usize = 2;
const WADE_DAMP: f32 = 0.7;
const MAX_DISPLACED: usize = 16;
const SCATTER_RADIUS: i32 = 6;

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
    var_jump_timer: f32,
    var_jump_speed: f32,
    max_fall: f32,
    ducking: bool,
    in_water: bool,
}

impl Controller {
    pub fn ducking(&self) -> bool {
        self.ducking
    }
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerParams {
    pub max_run: f32,
    pub run_accel: f32,
    pub run_reduce: f32,
    pub air_mult: f32,
    pub duck_friction: f32,
    pub gravity: f32,
    pub half_grav_threshold: f32,
    pub max_fall: f32,
    pub fast_max_fall: f32,
    pub fast_max_accel: f32,
    pub jump_speed: f32,
    pub jump_h_boost: f32,
    pub stand_half_h: f32,
    pub duck_half_h: f32,
    pub swim_max: f32,
    pub swim_underwater_max: f32,
    pub swim_accel: f32,
    pub swim_reduce: f32,
    pub swim_max_rise: f32,
}

impl Default for PlayerParams {
    fn default() -> Self {
        Self {
            max_run: 90.0,
            run_accel: 1000.0,
            run_reduce: 400.0,
            air_mult: 0.65,
            duck_friction: 500.0,
            gravity: 900.0,
            half_grav_threshold: 40.0,
            max_fall: 160.0,
            fast_max_fall: 240.0,
            fast_max_accel: 300.0,
            jump_speed: 105.0,
            jump_h_boost: 40.0,
            stand_half_h: 5.5,
            duck_half_h: 3.0,
            swim_max: 80.0,
            swim_underwater_max: 60.0,
            swim_accel: 600.0,
            swim_reduce: 400.0,
            swim_max_rise: 60.0,
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
    ctrl.coyote = if body.on_ground {
        COYOTE_SECS
    } else {
        (ctrl.coyote - dt).max(0.0)
    };
    ctrl.var_jump_timer = (ctrl.var_jump_timer - dt).max(0.0);

    let move_x = move_x.clamp(-1, 1) as f32;
    let in_water = body_submerged(world, registry, body);
    let entered_water = in_water && !ctrl.in_water;
    ctrl.in_water = in_water;
    let swimming = in_water && !(ctrl.var_jump_timer > 0.0 && body.vy > 0.0);
    if swimming {
        swim_update(
            world,
            registry,
            params,
            body,
            ctrl,
            move_x,
            jump_held,
            down_held,
            entered_water,
            dt,
        );
    } else {
        normal_update(
            world, registry, params, body, ctrl, move_x, jump_held, down_held, dt,
        );
    }

    let result = move_body(world, registry, body, dt);
    if result.hit_ceiling && ctrl.var_jump_timer < CEILING_VAR_JUMP_GRACE {
        ctrl.var_jump_timer = 0.0;
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn normal_update<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    params: &PlayerParams,
    body: &mut Body,
    ctrl: &mut Controller,
    move_x: f32,
    jump_held: bool,
    down_held: bool,
    dt: f32,
) {
    if !ctrl.ducking && down_held {
        duck(params, body, ctrl);
    } else if ctrl.ducking && !down_held && can_unduck(world, registry, params, body) {
        unduck(params, body, ctrl);
    }

    if body.on_ground && ctrl.ducking {
        body.vx = approach(body.vx, 0.0, params.duck_friction * dt);
    } else {
        let mult = if body.on_ground { 1.0 } else { params.air_mult };
        let target = move_x * params.max_run;
        let rate = if move_x != 0.0 && body.vx * move_x > 0.0 && body.vx.abs() > params.max_run {
            params.run_reduce
        } else {
            params.run_accel
        };
        body.vx = approach(body.vx, target, rate * mult * dt);
    }

    ctrl.max_fall = ctrl.max_fall.max(params.max_fall);
    let fast = down_held && body.vy <= -params.max_fall;
    let fall_target = if fast {
        params.fast_max_fall
    } else {
        params.max_fall
    };
    ctrl.max_fall = approach(ctrl.max_fall, fall_target, params.fast_max_accel * dt);

    if !body.on_ground {
        let gmult = if body.vy.abs() <= params.half_grav_threshold && jump_held {
            0.5
        } else {
            1.0
        };
        body.vy = approach(body.vy, -ctrl.max_fall, params.gravity * gmult * dt);
    }

    if ctrl.var_jump_timer > 0.0 {
        if jump_held {
            body.vy = body.vy.max(ctrl.var_jump_speed);
        } else {
            ctrl.var_jump_timer = 0.0;
        }
    }

    if ctrl.buffer > 0.0 && ctrl.coyote > 0.0 {
        jump(params, body, ctrl, move_x);
    }
}

#[allow(clippy::too_many_arguments)]
fn swim_update<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    params: &PlayerParams,
    body: &mut Body,
    ctrl: &mut Controller,
    move_x: f32,
    jump_held: bool,
    down_held: bool,
    entered_water: bool,
    dt: f32,
) {
    if entered_water && body.vy < 0.0 {
        body.vy *= SWIM_BEGIN_DAMP;
    }
    if ctrl.ducking && can_unduck(world, registry, params, body) {
        unduck(params, body, ctrl);
    }
    let probe = CellPos::new(
        body.x.floor() as i32,
        (body.y - body.half_h + UNDERWATER_PROBE_ABOVE_FEET).floor() as i32,
    );
    let underwater = cell_liquid(world, registry, probe);
    if !underwater && ctrl.buffer > 0.0 {
        jump(params, body, ctrl, move_x);
        return;
    }

    let max_x = if underwater {
        params.swim_underwater_max
    } else {
        params.swim_max
    };
    let rate_x = if move_x != 0.0 && body.vx * move_x > 0.0 && body.vx.abs() > max_x {
        params.swim_reduce
    } else {
        params.swim_accel
    };
    body.vx = approach(body.vx, move_x * max_x, rate_x * dt);

    let move_y = (jump_held as i8 - down_held as i8) as f32;
    if move_y != 0.0 {
        let rate_y = if body.vy * move_y > 0.0 && body.vy.abs() > params.swim_max {
            params.swim_reduce
        } else {
            params.swim_accel
        };
        body.vy = approach(body.vy, move_y * params.swim_max, rate_y * dt);
    } else if !underwater {
        body.vy = approach(body.vy, params.swim_max_rise, params.swim_accel * dt);
    } else {
        body.vy = approach(body.vy, 0.0, params.swim_accel * dt);
    }
}

fn jump(params: &PlayerParams, body: &mut Body, ctrl: &mut Controller, move_x: f32) {
    ctrl.buffer = 0.0;
    ctrl.coyote = 0.0;
    body.vx += params.jump_h_boost * move_x;
    body.vy = params.jump_speed;
    ctrl.var_jump_timer = VAR_JUMP_TIME;
    ctrl.var_jump_speed = body.vy;
}

fn duck(params: &PlayerParams, body: &mut Body, ctrl: &mut Controller) {
    body.y -= params.stand_half_h - params.duck_half_h;
    body.half_h = params.duck_half_h;
    ctrl.ducking = true;
}

fn unduck(params: &PlayerParams, body: &mut Body, ctrl: &mut Controller) {
    body.y += params.stand_half_h - body.half_h;
    body.half_h = params.stand_half_h;
    ctrl.ducking = false;
}

fn can_unduck<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    params: &PlayerParams,
    body: &Body,
) -> bool {
    let stand_cy = body.y - body.half_h + params.stand_half_h;
    let cur = cell_bounds(body.x, body.y, body.half_w, body.half_h);
    let (x0, y0, x1, y1) = cell_bounds(body.x, stand_cy, body.half_w, params.stand_half_h);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let overlapped = x >= cur.0 && x <= cur.2 && y >= cur.1 && y <= cur.3;
            if !overlapped && cell_blocks(world, registry, CellPos::new(x, y)) {
                return false;
            }
        }
    }
    true
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
    pub hit_ceiling: bool,
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

    fn step_top(&self) -> Option<i32> {
        if self.solid && self.solids.is_empty() {
            return None;
        }
        self.solids
            .iter()
            .chain(self.powder.iter())
            .map(|pos| pos.y)
            .max()
    }

    fn near_col(&self, dir: i32) -> Option<i32> {
        let cols = self
            .solids
            .iter()
            .chain(self.powder.iter())
            .map(|pos| pos.x);
        if dir > 0 { cols.min() } else { cols.max() }
    }

    fn near_row(&self, dir: i32) -> Option<i32> {
        let rows = self
            .solids
            .iter()
            .chain(self.powder.iter())
            .map(|pos| pos.y);
        if dir > 0 { rows.min() } else { rows.max() }
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

fn try_step_up<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &mut Body,
    blockage: &Blockage,
    dir: i32,
    target: &mut f32,
) -> Option<bool> {
    let step_top = blockage.step_top()?;
    let rise_needed = (step_top + 1) as f32 - (body.y - body.half_h);
    if rise_needed <= 0.0 || rise_needed > STEP_UP_CELLS as f32 {
        return None;
    }
    let near = blockage.near_col(dir)?;
    let dirf = dir as f32;
    let flush_x = if dir > 0 {
        near as f32 - body.half_w
    } else {
        (near + 1) as f32 + body.half_w
    };
    let budget = ((*target - flush_x) * dirf).max(0.0);
    let rise = rise_needed.min(budget);
    if rise <= 0.0 {
        body.x = flush_x;
        return Some(false);
    }
    if rect_blocked(world, registry, body, flush_x, body.y + rise) {
        return None;
    }
    body.x = flush_x;
    body.y += rise;
    *target = flush_x + dirf * (budget - rise);
    Some(rise + SKIN >= rise_needed)
}

fn corner_correct<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Body,
    next_y: f32,
    displaced: &[CellPos],
) -> Option<f32> {
    let mut sides: Vec<f32> = Vec::new();
    if body.vx <= 0.01 {
        sides.push(-1.0);
    }
    if body.vx >= -0.01 {
        sides.push(1.0);
    }
    for side in sides {
        for off in 1..=UPWARD_CORNER_CORRECTION {
            let cand_x = body.x + side * off as f32;
            if passage(world, registry, body, cand_x, next_y, displaced).free() {
                return Some(cand_x);
            }
        }
    }
    None
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

    let mut climbed = false;
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
                    break;
                }
                if let Some(cleared) =
                    try_step_up(world, registry, body, &blockage, dir, &mut target)
                {
                    climbed = true;
                    if cleared {
                        continue;
                    }
                    break;
                }
                result.record_blocked(&blockage.solids, body.vx, 0.0);
                body.vx = 0.0;
                break;
            }
            let blockage = passage(world, registry, body, next_x, body.y, &result.displaced);
            if blockage.free() {
                body.x = next_x;
                col = next_col;
                continue;
            }
            if let Some(cleared) = try_step_up(world, registry, body, &blockage, dir, &mut target) {
                climbed = true;
                if cleared {
                    continue;
                }
                break;
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
            let snap = blockage.near_col(dir);
            blockage.dig(&mut result.displaced);
            body.x = match snap {
                Some(near) if dir > 0 => near as f32 - body.half_w,
                Some(near) => (near + 1) as f32 + body.half_w,
                None if dir > 0 => next_col as f32 - body.half_w,
                None => (next_col + 1) as f32 + body.half_w,
            };
            body.vx = 0.0;
            break;
        }
    }

    if climbed && was_grounded {
        body.on_ground = true;
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
                    if dir > 0 {
                        result.hit_ceiling = true;
                    }
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
                if dir > 0 {
                    if let Some(corrected_x) =
                        corner_correct(world, registry, body, next_y, &result.displaced)
                    {
                        body.x = corrected_x;
                        body.y = next_y;
                        row = next_row;
                        continue;
                    }
                    result.hit_ceiling = true;
                }
                result.record_blocked(&blockage.solids, 0.0, body.vy);
                body.y = match blockage.near_row(dir) {
                    Some(near) if dir > 0 => near as f32 - body.half_h,
                    Some(near) => (near + 1) as f32 + body.half_h,
                    None if dir > 0 => next_row as f32 - body.half_h,
                    None => (next_row + 1) as f32 + body.half_h,
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
