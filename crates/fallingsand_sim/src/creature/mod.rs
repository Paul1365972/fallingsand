mod player;
mod sweep;

pub use player::{Controller, PlayerParams, step_player};
pub use sweep::{Blocked, MoveResult};

use crate::shape::{CellSource, Footprint, OwnCells, Shape};
use fallingsand_core::content;
use fallingsand_core::{CellPos, Motion, Phase, Subcell, TICK_DT, TICK_RATE};
use fallingsand_math::SUBCELL_UNITS_PER_CELL;

pub(crate) const BOUNCE_MIN_SPEED: f32 = 30.0;
const FLUID_DRAG_LINEAR: f32 = 2.5;
const FLUID_DRAG_QUAD: f32 = 0.0625;
const MAX_FLUID_DRAG: f32 = 0.9;

pub(crate) fn fluid_drag(speed: f32, submersion: f32) -> f32 {
    ((FLUID_DRAG_LINEAR + FLUID_DRAG_QUAD * speed) * submersion * TICK_DT).min(MAX_FLUID_DRAG)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Creature {
    pub x: Subcell,
    pub y: Subcell,
    pub vx: Subcell,
    pub vy: Subcell,
    pub shape: Shape,
    pub climb_debt: Subcell,
    pub on_ground: bool,
}

impl Creature {
    pub fn new(x: Subcell, y: Subcell, shape: Shape) -> Self {
        Self {
            x,
            y,
            vx: Subcell::ZERO,
            vy: Subcell::ZERO,
            shape,
            climb_debt: Subcell::ZERO,
            on_ground: false,
        }
    }

    pub fn cell(&self) -> CellPos {
        CellPos::new(self.x.floor_cell(), self.y.floor_cell())
    }

    pub fn origin(&self) -> (i32, i32) {
        self.shape.origin(self.x, self.y)
    }

    pub fn footprint(&self) -> Footprint {
        self.shape.footprint(self.x, self.y)
    }

    pub fn rows(&self) -> i32 {
        self.shape.h()
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

pub fn grounded<W: CellSource>(world: &W, body: &Creature, own: OwnCells) -> bool {
    body.vy <= Subcell::ZERO && body.shape.supported_at(world, own, body.origin())
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BodySubmersion {
    pub fraction: f32,
    pub liquid_density: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Submersion {
    fraction: f32,
    liquid_density: f32,
    flow_vx: f32,
    flow_vy: f32,
}

fn body_submersion<W: CellSource>(
    world: &W,
    body: &Creature,
    displaced: BodySubmersion,
) -> Submersion {
    if displaced.fraction <= 0.0 {
        return Submersion::default();
    }
    let (flow_vx, flow_vy) = ring_flow(world, body);
    Submersion {
        fraction: displaced.fraction,
        liquid_density: displaced.liquid_density,
        flow_vx,
        flow_vy,
    }
}

fn ring_flow<W: CellSource>(world: &W, body: &Creature) -> (f32, f32) {
    let fp = body.footprint();
    let mut liquid = 0u32;
    let mut flow_x = 0i64;
    let mut flow_y = 0i64;
    let mut sample = |pos: CellPos| {
        let Some(cell) = world.cell_at(pos) else {
            return;
        };
        if content::phase(cell.material) == Phase::Liquid
            && let Motion::Velocity(cvx, cvy) = cell.motion()
        {
            liquid += 1;
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
        sample(CellPos::new(x, fp.y1 + 1));
    }
    if liquid == 0 {
        return (0.0, 0.0);
    }
    let per_cell = 1.0 / liquid as f32;
    let to_per_sec = TICK_RATE as f32 / SUBCELL_UNITS_PER_CELL as f32;
    (
        flow_x as f32 * per_cell * to_per_sec,
        flow_y as f32 * per_cell * to_per_sec,
    )
}
