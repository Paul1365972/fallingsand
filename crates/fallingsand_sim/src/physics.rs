use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, Fixed, MaterialRegistry, Phase, TICK_DT, VEL_ONE};
use rustc_hash::FxHashSet;

pub const BOUNCE_MIN_SPEED: f32 = 30.0;
const LAUNCH_MIN_SPEED: Fixed = Fixed::from_int(80);
const LEDGE_LAUNCH_K: Fixed = Fixed::from_f32(0.35);
const MIN_GRIP: f32 = 0.06;

pub const STEP_UP_CELLS: i32 = 3;
pub const STEP_DOWN_CELLS: i32 = 3;
const COYOTE_SECS: f32 = 0.1;
const BUFFER_SECS: f32 = 0.1;
const VAR_JUMP_TIME: f32 = 0.2;
const CEILING_VAR_JUMP_GRACE: f32 = 0.15;
const UPWARD_CORNER_CORRECTION: i32 = 4;
pub const FLUID_DRAG_LINEAR: f32 = 2.5;
pub const FLUID_DRAG_QUAD: f32 = 0.0625;
pub const MAX_FLUID_DRAG: f32 = 0.9;
const SNAP_DOWN_MAX_SUBMERSION: f32 = 0.5;
const SWIM_CONTROL_MIN_SUBMERSION: f32 = 0.5;
const BANK_VAULT_MIN_SUBMERSION: f32 = 0.2;
const BANK_VAULT_MAX_SUBMERSION: f32 = 0.95;
const BANK_VAULT_MAX_SINK: Fixed = Fixed::from_int(20);
const BANK_PROBE_CELLS: i32 = 3;
const CLIMB_COST: Fixed = Fixed::HALF;
const CLIMB_DRAIN: Fixed = Fixed::HALF;

pub trait CellSource {
    fn cell_at(&self, pos: CellPos) -> Option<Cell>;
}

impl CellSource for CellWorld {
    fn cell_at(&self, pos: CellPos) -> Option<Cell> {
        self.get_cell(pos)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub x: Fixed,
    pub y: Fixed,
    pub vx: Fixed,
    pub vy: Fixed,
    pub half_w: Fixed,
    pub half_h: Fixed,
    pub climb_debt: Fixed,
    pub on_ground: bool,
}

impl Actor {
    pub fn new(x: Fixed, y: Fixed, half_w: Fixed, half_h: Fixed) -> Self {
        Self {
            x,
            y,
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            half_w,
            half_h,
            climb_debt: Fixed::ZERO,
            on_ground: false,
        }
    }

    pub fn cell(&self) -> CellPos {
        CellPos::new(self.x.floor_cell(), self.y.floor_cell())
    }

    pub fn footprint(&self) -> Footprint {
        footprint_at(self.x, self.y, self.half_w, self.half_h)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Footprint {
    pub fn contains(&self, pos: CellPos) -> bool {
        pos.x >= self.x0 && pos.x <= self.x1 && pos.y >= self.y0 && pos.y <= self.y1
    }
}

pub fn footprint_at(cx: Fixed, cy: Fixed, half_w: Fixed, half_h: Fixed) -> Footprint {
    let w = half_w.mul_int(2).round_int().max(1);
    let h = half_h.mul_int(2).round_int().max(1);
    let x0 = cx.floor_cell() - w / 2;
    let y0 = cy.floor_cell() - h / 2;
    Footprint {
        x0,
        y0,
        x1: x0 + w - 1,
        y1: y0 + h - 1,
    }
}

pub type OwnCells<'a> = Option<&'a FxHashSet<CellPos>>;

fn own_covers(own: OwnCells, pos: CellPos) -> bool {
    own.is_some_and(|set| set.contains(&pos))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorAabb {
    pub x: Fixed,
    pub y: Fixed,
    pub half_w: Fixed,
    pub half_h: Fixed,
}

impl ActorAabb {
    pub fn contains_cell(&self, pos: CellPos) -> bool {
        let (cx, cy) = (Fixed::cell_center(pos.x), Fixed::cell_center(pos.y));
        (cx - self.x).abs() <= self.half_w && (cy - self.y).abs() <= self.half_h
    }

    pub fn from_footprint(fp: Footprint) -> Self {
        let half_w = Fixed::from_int(fp.x1 - fp.x0 + 1).mul(Fixed::HALF);
        let half_h = Fixed::from_int(fp.y1 - fp.y0 + 1).mul(Fixed::HALF);
        Self {
            x: Fixed::from_cell(fp.x0) + half_w,
            y: Fixed::from_cell(fp.y0) + half_h,
            half_w,
            half_h,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StepInput {
    pub move_x: i8,
    pub jump: bool,
    pub jump_pressed: bool,
    pub down: bool,
    pub fly: bool,
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

fn rect_blocked<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
    cx: Fixed,
    cy: Fixed,
    own: OwnCells,
) -> bool {
    let cur = body.footprint();
    let next = footprint_at(cx, cy, body.half_w, body.half_h);
    for y in next.y0..=next.y1 {
        for x in next.x0..=next.x1 {
            let pos = CellPos::new(x, y);
            if cur.contains(pos) || own_covers(own, pos) {
                continue;
            }
            if cell_blocks(world, registry, pos) {
                return true;
            }
        }
    }
    false
}

fn supported_at<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
    cx: Fixed,
    cy: Fixed,
    own: OwnCells,
) -> bool {
    let next = footprint_at(cx, cy, body.half_w, body.half_h);
    let row = next.y0 - 1;
    (next.x0..=next.x1).any(|x| {
        let pos = CellPos::new(x, row);
        !own_covers(own, pos) && cell_blocks(world, registry, pos)
    })
}

pub fn grounded<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
    own: OwnCells,
) -> bool {
    body.vy <= Fixed::ZERO && supported_at(world, registry, body, body.x, body.y, own)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Submersion {
    pub fraction: f32,
    pub liquid_density: f32,
    pub flow_vx: f32,
    pub flow_vy: f32,
}

pub fn body_submersion<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
) -> Submersion {
    let fp = body.footprint();
    let mut total = 0u32;
    let mut liquid = 0u32;
    let mut density_sum = 0.0f32;
    let mut flow_x = 0i64;
    let mut flow_y = 0i64;
    for y in fp.y0..=fp.y1 {
        for x in fp.x0..=fp.x1 {
            total += 1;
            let Some(cell) = world.cell_at(CellPos::new(x, y)) else {
                continue;
            };
            let material = registry.get(cell.material);
            if material.phase == Phase::Liquid {
                liquid += 1;
                density_sum += material.density;
                let (cvx, cvy) = cell.vel();
                flow_x += cvx as i64;
                flow_y += cvy as i64;
            }
        }
    }
    if liquid == 0 {
        return Submersion::default();
    }
    let per_cell = 1.0 / liquid as f32;
    let to_per_sec = 1.0 / VEL_ONE as f32;
    Submersion {
        fraction: liquid as f32 / total as f32,
        liquid_density: density_sum / liquid as f32,
        flow_vx: flow_x as f32 * per_cell * to_per_sec,
        flow_vy: flow_y as f32 * per_cell * to_per_sec,
    }
}

pub fn ring_submersion<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
) -> Submersion {
    let fp = body.footprint();
    let mut total = 0u32;
    let mut liquid = 0u32;
    let mut density_sum = 0.0f32;
    let mut flow_x = 0i64;
    let mut flow_y = 0i64;
    let mut sample = |pos: CellPos| {
        total += 1;
        let Some(cell) = world.cell_at(pos) else {
            return;
        };
        let material = registry.get(cell.material);
        if material.phase == Phase::Liquid {
            liquid += 1;
            density_sum += material.density;
            let (cvx, cvy) = cell.vel();
            flow_x += cvx as i64;
            flow_y += cvy as i64;
        }
    };
    for y in fp.y0..=fp.y1 {
        sample(CellPos::new(fp.x0 - 1, y));
        sample(CellPos::new(fp.x1 + 1, y));
    }
    for x in fp.x0..=fp.x1 {
        sample(CellPos::new(x, fp.y0 - 1));
    }
    if liquid == 0 {
        return Submersion::default();
    }
    let per_cell = 1.0 / liquid as f32;
    let to_per_sec = 1.0 / VEL_ONE as f32;
    Submersion {
        fraction: liquid as f32 / total as f32,
        liquid_density: density_sum / liquid as f32,
        flow_vx: flow_x as f32 * per_cell * to_per_sec,
        flow_vy: flow_y as f32 * per_cell * to_per_sec,
    }
}

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

fn approach(value: Fixed, target: Fixed, delta: Fixed) -> Fixed {
    if value < target {
        (value + delta).min(target)
    } else {
        (value - delta).max(target)
    }
}

fn resolve_axis(v: Fixed, e: f32) -> Fixed {
    if v.abs() > Fixed::from_f32(BOUNCE_MIN_SPEED) {
        -v.mul(Fixed::from_f32(e))
    } else {
        Fixed::ZERO
    }
}

fn solids_bounce<W: CellSource>(world: &W, registry: &MaterialRegistry, solids: &[CellPos]) -> f32 {
    let mut e = 0.0f32;
    for &pos in solids {
        if let Some(cell) = world.cell_at(pos) {
            e = e.max(registry.get(cell.material).surface_bounce);
        }
    }
    e
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

fn same_direction(v: Fixed, dir: i32) -> bool {
    (dir > 0 && v > Fixed::ZERO) || (dir < 0 && v < Fixed::ZERO)
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blocked {
    pub pos: CellPos,
    pub dvx: f32,
    pub dvy: f32,
}

#[derive(Debug, Default)]
pub struct MoveResult {
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
}

impl Blockage {
    fn free(&self) -> bool {
        !self.solid
    }

    fn step_top(&self) -> Option<i32> {
        self.solids.iter().map(|pos| pos.y).max()
    }

    fn near_col(&self, dir: i32) -> Option<i32> {
        let cols = self.solids.iter().map(|pos| pos.x);
        if dir > 0 { cols.min() } else { cols.max() }
    }

    fn near_row(&self, dir: i32) -> Option<i32> {
        let rows = self.solids.iter().map(|pos| pos.y);
        if dir > 0 { rows.min() } else { rows.max() }
    }
}

fn passage<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
    cx: Fixed,
    cy: Fixed,
    own: OwnCells,
) -> Blockage {
    let cur = body.footprint();
    let next = footprint_at(cx, cy, body.half_w, body.half_h);
    let mut blockage = Blockage {
        solid: false,
        solids: Vec::new(),
    };
    for y in next.y0..=next.y1 {
        for x in next.x0..=next.x1 {
            let pos = CellPos::new(x, y);
            let Some(cell) = world.cell_at(pos) else {
                blockage.solid = true;
                continue;
            };
            if cur.contains(pos) || own_covers(own, pos) {
                continue;
            }
            if matches!(
                registry.get(cell.material).phase,
                Phase::Solid | Phase::Powder
            ) {
                blockage.solid = true;
                blockage.solids.push(pos);
            }
        }
    }
    blockage
}

fn try_step_up<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &mut Actor,
    blockage: &Blockage,
    own: OwnCells,
) -> bool {
    let Some(step_top) = blockage.step_top() else {
        return false;
    };
    let fp = body.footprint();
    let down = body.y.floor_cell() - fp.y0;
    let rise_needed = Fixed::from_cell(step_top + 1 + down) - body.y;
    if rise_needed <= Fixed::ZERO || rise_needed > Fixed::from_int(STEP_UP_CELLS) {
        return false;
    }
    if rect_blocked(world, registry, body, body.x, body.y + rise_needed, own) {
        return false;
    }
    body.y += rise_needed;
    body.climb_debt += rise_needed.mul(CLIMB_COST);
    if body.vx.abs() > LAUNCH_MIN_SPEED {
        body.vy = body.vy.max(body.vx.abs().mul(LEDGE_LAUNCH_K));
    }
    true
}

fn corner_correct<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &Actor,
    next_y: Fixed,
    own: OwnCells,
) -> Option<Fixed> {
    let mut sides: Vec<i32> = Vec::new();
    if body.vx <= Fixed::ZERO {
        sides.push(-1);
    }
    if body.vx >= Fixed::ZERO {
        sides.push(1);
    }
    for side in sides {
        for off in 1..=UPWARD_CORNER_CORRECTION {
            let cand_x = body.x + Fixed::from_int(side * off);
            if passage(world, registry, body, cand_x, next_y, own).free() {
                return Some(cand_x);
            }
        }
    }
    None
}

pub fn move_body<W: CellSource>(
    world: &W,
    registry: &MaterialRegistry,
    body: &mut Actor,
    submersion: f32,
    own: OwnCells,
) -> MoveResult {
    let mut result = MoveResult::default();
    let was_grounded = body.on_ground;
    body.on_ground = false;
    let w = body.half_w.mul_int(2).round_int().max(1);
    let h = body.half_h.mul_int(2).round_int().max(1);
    let (w_left, w_right) = (w / 2, w - 1 - w / 2);
    let (h_down, h_up) = (h / 2, h - 1 - h / 2);
    let mut remaining_x = body.vx.per_tick();
    let remaining_y = body.vy.per_tick();

    if remaining_x == Fixed::ZERO {
        body.climb_debt = Fixed::ZERO;
    } else {
        let drain = body.climb_debt.mul(CLIMB_DRAIN).min(remaining_x.abs());
        body.climb_debt -= drain;
        remaining_x = if remaining_x > Fixed::ZERO {
            remaining_x - drain
        } else {
            remaining_x + drain
        };
    }

    let mut climbed = false;
    if remaining_x != Fixed::ZERO {
        let dir = if remaining_x > Fixed::ZERO { 1i32 } else { -1 };
        let target = body.x + remaining_x;
        let mut col = if dir > 0 {
            body.x.floor_cell() + w_right
        } else {
            body.x.floor_cell() - w_left
        };
        loop {
            let next_col = col + dir;
            let next_x = if dir > 0 {
                Fixed::from_cell(next_col - w_right)
            } else {
                Fixed::from_cell(next_col + w_left + 1) - Fixed::SUBUNIT
            };
            let overshoots = if dir > 0 {
                next_x >= target
            } else {
                next_x <= target
            };
            if overshoots {
                let blockage = passage(world, registry, body, target, body.y, own);
                if blockage.free() {
                    body.x = target;
                    break;
                }
                if try_step_up(world, registry, body, &blockage, own) {
                    climbed = true;
                    continue;
                }
                let e = solids_bounce(world, registry, &blockage.solids);
                let after = resolve_axis(body.vx, e);
                result.record_blocked(&blockage.solids, (body.vx - after).to_f32(), 0.0);
                body.vx = after;
                break;
            }
            let blockage = passage(world, registry, body, next_x, body.y, own);
            if blockage.free() {
                body.x = next_x;
                col = next_col;
                continue;
            }
            if try_step_up(world, registry, body, &blockage, own) {
                climbed = true;
                continue;
            }
            let e = solids_bounce(world, registry, &blockage.solids);
            let after = resolve_axis(body.vx, e);
            result.record_blocked(&blockage.solids, (body.vx - after).to_f32(), 0.0);
            let snap = blockage.near_col(dir);
            body.x = match snap {
                Some(near) if dir > 0 => Fixed::from_cell(near - w_right) - Fixed::SUBUNIT,
                Some(near) => Fixed::from_cell(near + 1 + w_left),
                None if dir > 0 => Fixed::from_cell(next_col - w_right) - Fixed::SUBUNIT,
                None => Fixed::from_cell(next_col + 1 + w_left),
            };
            body.vx = after;
            break;
        }
    }

    if climbed && was_grounded && body.vy <= Fixed::ZERO {
        body.on_ground = true;
    }

    if was_grounded
        && body.vy <= Fixed::ZERO
        && submersion < SNAP_DOWN_MAX_SUBMERSION
        && !supported_at(world, registry, body, body.x, body.y, own)
    {
        for step in 1..=STEP_DOWN_CELLS {
            let next_y = body.y - Fixed::from_int(step);
            if rect_blocked(world, registry, body, body.x, next_y, own) {
                break;
            }
            if supported_at(world, registry, body, body.x, next_y, own) {
                body.y = next_y;
                body.on_ground = true;
                break;
            }
        }
    }

    if remaining_y != Fixed::ZERO {
        let dir = if remaining_y > Fixed::ZERO { 1i32 } else { -1 };
        let target = body.y + remaining_y;
        let mut row = if dir > 0 {
            body.y.floor_cell() + h_up
        } else {
            body.y.floor_cell() - h_down
        };
        loop {
            let next_row = row + dir;
            let next_y = if dir > 0 {
                Fixed::from_cell(next_row - h_up)
            } else {
                Fixed::from_cell(next_row + h_down + 1) - Fixed::SUBUNIT
            };
            let overshoots = if dir > 0 {
                next_y >= target
            } else {
                next_y <= target
            };
            if overshoots {
                let blockage = passage(world, registry, body, body.x, target, own);
                if blockage.free() {
                    body.y = target;
                } else {
                    let e = solids_bounce(world, registry, &blockage.solids);
                    let after = resolve_axis(body.vy, e);
                    result.record_blocked(&blockage.solids, 0.0, (body.vy - after).to_f32());
                    if dir > 0 {
                        result.hit_ceiling = true;
                    }
                    body.vy = after;
                }
                break;
            }
            let blockage = passage(world, registry, body, body.x, next_y, own);
            if blockage.free() {
                body.y = next_y;
                row = next_row;
            } else {
                if dir > 0 {
                    if let Some(corrected_x) = corner_correct(world, registry, body, next_y, own) {
                        body.x = corrected_x;
                        body.y = next_y;
                        row = next_row;
                        continue;
                    }
                    result.hit_ceiling = true;
                }
                let e = solids_bounce(world, registry, &blockage.solids);
                let after = resolve_axis(body.vy, e);
                result.record_blocked(&blockage.solids, 0.0, (body.vy - after).to_f32());
                body.y = match blockage.near_row(dir) {
                    Some(near) if dir > 0 => Fixed::from_cell(near - h_up) - Fixed::SUBUNIT,
                    Some(near) => Fixed::from_cell(near + 1 + h_down),
                    None if dir > 0 => Fixed::from_cell(next_row - h_up) - Fixed::SUBUNIT,
                    None => Fixed::from_cell(next_row + 1 + h_down),
                };
                if dir < 0 && after <= Fixed::ZERO {
                    body.on_ground = true;
                }
                body.vy = after;
                break;
            }
        }
    }

    if body.vy <= Fixed::ZERO
        && !body.on_ground
        && supported_at(world, registry, body, body.x, body.y, own)
    {
        body.on_ground = true;
    }
    result
}
