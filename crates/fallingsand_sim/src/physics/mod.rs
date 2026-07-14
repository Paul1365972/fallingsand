mod movement;
mod player;

pub use movement::{Blocked, MoveResult};
pub use player::{Controller, PlayerParams, step_player};

use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{Cell, CellPos, Fixed, Phase, TICK_DT, TICK_RATE, VEL_ONE};
use rustc_hash::FxHashSet;

pub(crate) const BOUNCE_MIN_SPEED: f32 = 30.0;
const FLUID_DRAG_LINEAR: f32 = 2.5;
const FLUID_DRAG_QUAD: f32 = 0.0625;
const MAX_FLUID_DRAG: f32 = 0.9;

pub(crate) fn fluid_drag(speed: f32, submersion: f32) -> f32 {
    ((FLUID_DRAG_LINEAR + FLUID_DRAG_QUAD * speed) * submersion * TICK_DT).min(MAX_FLUID_DRAG)
}

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

    pub fn rows(&self) -> i32 {
        self.half_h.mul_int(2).round_int().max(1)
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

fn cell_blocks<W: CellSource>(world: &W, pos: CellPos) -> bool {
    match world.cell_at(pos) {
        Some(cell) => matches!(content::phase(cell.material), Phase::Solid | Phase::Powder),
        None => true,
    }
}

enum Obstacle {
    Unloaded,
    Solid(CellPos),
}

// Visits every candidate cell the footprint would newly occupy at (cx, cy) -- those
// outside the current footprint and not owned -- and reports each blocking cell.
// Allocation-free: callers early-exit (rect_blocked) or accumulate (passage) themselves.
fn walk_footprint<W: CellSource>(
    world: &W,
    body: &Actor,
    cx: Fixed,
    cy: Fixed,
    own: OwnCells,
    mut visit: impl FnMut(Obstacle) -> std::ops::ControlFlow<()>,
) {
    let cur = body.footprint();
    let next = footprint_at(cx, cy, body.half_w, body.half_h);
    for y in next.y0..=next.y1 {
        for x in next.x0..=next.x1 {
            let pos = CellPos::new(x, y);
            if cur.contains(pos) || own_covers(own, pos) {
                continue;
            }
            let obstacle = match world.cell_at(pos) {
                None => Obstacle::Unloaded,
                Some(cell)
                    if matches!(content::phase(cell.material), Phase::Solid | Phase::Powder) =>
                {
                    Obstacle::Solid(pos)
                }
                Some(_) => continue,
            };
            if visit(obstacle).is_break() {
                return;
            }
        }
    }
}

fn rect_blocked<W: CellSource>(
    world: &W,
    body: &Actor,
    cx: Fixed,
    cy: Fixed,
    own: OwnCells,
) -> bool {
    let mut blocked = false;
    walk_footprint(world, body, cx, cy, own, |_| {
        blocked = true;
        std::ops::ControlFlow::Break(())
    });
    blocked
}

fn supported_at<W: CellSource>(
    world: &W,
    body: &Actor,
    cx: Fixed,
    cy: Fixed,
    own: OwnCells,
) -> bool {
    let next = footprint_at(cx, cy, body.half_w, body.half_h);
    let row = next.y0 - 1;
    (next.x0..=next.x1).any(|x| {
        let pos = CellPos::new(x, row);
        !own_covers(own, pos) && cell_blocks(world, pos)
    })
}

pub fn grounded<W: CellSource>(world: &W, body: &Actor, own: OwnCells) -> bool {
    body.vy <= Fixed::ZERO && supported_at(world, body, body.x, body.y, own)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Submersion {
    fraction: f32,
    liquid_density: f32,
    flow_vx: f32,
    flow_vy: f32,
}

fn ring_submersion<W: CellSource>(world: &W, body: &Actor) -> Submersion {
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
        if content::phase(cell.material) == Phase::Liquid {
            liquid += 1;
            density_sum += content::density_milli(cell.material) as f32 / 1000.0;
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
    let to_per_sec = TICK_RATE as f32 / VEL_ONE as f32;
    Submersion {
        fraction: liquid as f32 / total as f32,
        liquid_density: density_sum / liquid as f32,
        flow_vx: flow_x as f32 * per_cell * to_per_sec,
        flow_vy: flow_y as f32 * per_cell * to_per_sec,
    }
}
