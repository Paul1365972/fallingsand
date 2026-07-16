mod contact;
mod island;
mod rotation;
mod step;

pub use island::{
    BodyParts, apply_damage, body_parts, detect_island, register_body, revive_body, stamp_raster,
    unstamp_body,
};
pub use step::{SETTLE_SECS, settle_body, step_bodies};

use crate::physics::ActorAabb;
use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{CARDINAL_NEIGHBORS as NEIGHBORS, Cell, CellPos, Fixed, MaterialId, Phase};
use rotation::{ANGLE_STEPS, ANGLE_STEPS_LARGE, LARGE_BODY_EXTENT, quantize_step, rotate_offset};
use rustc_hash::{FxHashMap, FxHashSet};

const REFERENCE_DENSITY_MILLI: f32 = 1_000_000.0;
const RELOCATE_RADIUS: i32 = 8;
const SURFACE_PROBE: i32 = 64;

fn cell_mass(material: MaterialId) -> f32 {
    content::density_milli(material) as f32 / REFERENCE_DENSITY_MILLI
}

#[derive(Debug, Clone, Copy)]
pub struct ActorDynamics {
    pub bbox: ActorAabb,
    pub vx: f32,
    pub vy: f32,
    pub inv_mass: f32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Raster {
    pub(crate) cells: Vec<(CellPos, u16)>,
    pub(crate) set: FxHashSet<CellPos>,
}

impl Raster {
    pub(crate) fn covers(&self, pos: CellPos) -> bool {
        self.set.contains(&pos)
    }
}

#[derive(Debug, Clone, Default)]
pub struct OwnerMap {
    owner: FxHashMap<CellPos, usize>,
}

impl OwnerMap {
    pub fn rebuild(&mut self, bodies: &[PixelBody]) {
        self.owner.clear();
        for (index, body) in bodies.iter().enumerate() {
            for &pos in &body.raster.set {
                self.owner.insert(pos, index);
            }
        }
    }

    pub fn get(&self, pos: CellPos) -> Option<usize> {
        self.owner.get(&pos).copied()
    }

    fn reseat(&mut self, index: usize, old: &Raster, new: &Raster) {
        for pos in old.set.difference(&new.set) {
            if self.owner.get(pos) == Some(&index) {
                self.owner.remove(pos);
            }
        }
        for &pos in &new.set {
            self.owner.insert(pos, index);
        }
    }
}

#[derive(Debug, Clone)]
pub struct PixelBody {
    pub id: u32,
    pub(crate) width: u8,
    pub(crate) height: u8,
    pub(crate) cells: Vec<Cell>,
    pub(crate) perimeter: Vec<(u8, u8)>,
    pub(crate) com_local: (f32, f32),
    pub(crate) pivot: (i32, i32),
    pub(crate) angle_steps: u32,
    pub x: Fixed,
    pub y: Fixed,
    pub vx: Fixed,
    pub vy: Fixed,
    pub angle: f32,
    pub spin: f32,
    pub(crate) inv_mass: f32,
    pub(crate) inv_inertia: f32,
    pub restitution: f32,
    pub rest_secs: f32,
    pub(crate) raster: Raster,
    pub frozen: bool,
}

pub fn wake_covering(bodies: &mut [PixelBody], owners: &OwnerMap, pos: CellPos) {
    if let Some(body) = owners.get(pos).and_then(|index| bodies.get_mut(index)) {
        body.rest_secs = 0.0;
    }
}

pub fn vacated_wake_targets(
    world: &CellWorld,
    covers: &dyn Fn(CellPos) -> bool,
    vacated: &[CellPos],
) -> Vec<CellPos> {
    let mut targets = Vec::new();
    for &pos in vacated {
        for (dx, dy) in NEIGHBORS {
            let neighbor = pos.translated(dx, dy);
            if covers(neighbor) {
                continue;
            }
            if world.get_cell(neighbor).is_some_and(|cell| cell.is_body()) {
                targets.push(neighbor);
            }
        }
    }
    targets
}

impl PixelBody {
    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    pub fn inv_mass(&self) -> f32 {
        self.inv_mass
    }

    pub fn inv_inertia(&self) -> f32 {
        self.inv_inertia
    }

    pub fn covers(&self, pos: CellPos) -> bool {
        self.raster.covers(pos)
    }

    fn offset_with(&self, sin: f32, cos: f32, lx: f32, ly: f32) -> (f32, f32) {
        let (dx, dy) = (lx - self.com_local.0, ly - self.com_local.1);
        (dx * cos - dy * sin, dx * sin + dy * cos)
    }

    fn local_offset(&self, lx: f32, ly: f32) -> (f32, f32) {
        let (sin, cos) = quantized_trig_of(self.angle, self.angle_steps);
        self.offset_with(sin, cos, lx, ly)
    }

    fn pivot_cell(&self, x: Fixed, y: Fixed) -> CellPos {
        let (px, py) = self.pivot;
        let ox = px as f32 + 0.5 - self.com_local.0;
        let oy = py as f32 + 0.5 - self.com_local.1;
        CellPos::new(x.add_f32(ox).floor_cell(), y.add_f32(oy).floor_cell())
    }

    fn body_cell(&self, pivot_cell: CellPos, step: u32, lx: u8, ly: u8) -> CellPos {
        let (dx, dy) = rotate_offset(
            step,
            self.angle_steps,
            lx as i32 - self.pivot.0,
            ly as i32 - self.pivot.1,
        );
        pivot_cell.translated(dx, dy)
    }
}

fn quantized_trig_of(angle: f32, steps: u32) -> (f32, f32) {
    let step = quantize_step(angle, steps);
    (step as f32 / steps as f32 * std::f32::consts::TAU).sin_cos()
}

fn angle_steps_for(width: u8, height: u8) -> u32 {
    if width.max(height) as i32 >= LARGE_BODY_EXTENT {
        ANGLE_STEPS_LARGE
    } else {
        ANGLE_STEPS
    }
}

fn rasterize_at(body: &PixelBody, x: Fixed, y: Fixed, angle: f32) -> Raster {
    let step = quantize_step(angle, body.angle_steps);
    let pivot_cell = body.pivot_cell(x, y);
    let mut raster = Raster::default();
    for ly in 0..body.height {
        for lx in 0..body.width {
            let index = ly as usize * body.width as usize + lx as usize;
            if body.cells[index].is_air() {
                continue;
            }
            let pos = body.body_cell(pivot_cell, step, lx, ly);
            raster.set.insert(pos);
            raster.cells.push((pos, index as u16));
        }
    }
    raster
}

pub(crate) fn commit_stamp(
    world: &mut CellWorld,
    entities: &[ActorAabb],
    old: &Raster,
    new: &Raster,
    cell_for: &dyn Fn(u16) -> Cell,
) -> Option<Vec<CellPos>> {
    let mut displaced: Vec<(CellPos, Cell)> = Vec::new();
    for &(pos, _) in &new.cells {
        if old.covers(pos) {
            continue;
        }
        let cell = world.get_cell(pos)?;
        if cell.is_body() {
            return None;
        }
        match content::phase(cell.material) {
            Phase::Solid | Phase::Powder => return None,
            Phase::Empty => {}
            Phase::Liquid | Phase::Gas => displaced.push((pos, cell)),
        }
        if entities.iter().any(|entity| entity.contains_cell(pos)) {
            return None;
        }
    }

    let mut vacated: Vec<CellPos> = old
        .set
        .iter()
        .filter(|pos| !new.set.contains(pos))
        .copied()
        .collect();
    vacated.sort_unstable_by_key(|pos| (pos.y, pos.x));
    displaced.sort_unstable_by_key(|&(pos, _)| (pos.y, pos.x));

    let mut writes: Vec<(CellPos, Cell)> = Vec::new();
    let mut claimed: FxHashSet<CellPos> = FxHashSet::default();
    let mut receptacles = vacated.iter();
    let mut spill: Vec<(CellPos, Cell)> = Vec::new();
    for &(pos, cell) in &displaced {
        match receptacles.next() {
            Some(&target) => writes.push((target, cell)),
            None => spill.push((pos, cell)),
        }
    }
    for &target in receptacles {
        writes.push((target, Cell::AIR));
    }
    for (from, cell) in spill {
        let spot = relocation_spot(world, entities, &claimed, &new.set, from)?;
        claimed.insert(spot);
        writes.push((spot, cell));
    }

    for (pos, cell) in writes {
        world.set_cell_raw(pos, cell);
    }
    for &(pos, local) in &new.cells {
        world.set_cell_raw(pos, cell_for(local));
    }
    Some(vacated)
}

fn chebyshev_ring(radius: i32) -> Vec<(i32, i32)> {
    let mut ring = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx.abs().max(dy.abs()) == radius {
                ring.push((dx, dy));
            }
        }
    }
    ring
}

fn relocation_spot(
    world: &CellWorld,
    entities: &[ActorAabb],
    claimed: &FxHashSet<CellPos>,
    exclude: &FxHashSet<CellPos>,
    from: CellPos,
) -> Option<CellPos> {
    for radius in 1..=RELOCATE_RADIUS {
        let mut ring = chebyshev_ring(radius);
        ring.sort_by_key(|&(dx, dy)| (-dy, dx.abs()));
        for (dx, dy) in ring {
            let pos = from.translated(dx, dy);
            if claimed.contains(&pos)
                || exclude.contains(&pos)
                || entities.iter().any(|entity| entity.contains_cell(pos))
            {
                continue;
            }
            let empty = world
                .get_cell(pos)
                .is_some_and(|cell| content::phase(cell.material) == Phase::Empty);
            if empty {
                return Some(pos);
            }
        }
    }
    surface_spot(world, entities, claimed, exclude, from)
}

fn surface_spot(
    world: &CellWorld,
    entities: &[ActorAabb],
    claimed: &FxHashSet<CellPos>,
    exclude: &FxHashSet<CellPos>,
    from: CellPos,
) -> Option<CellPos> {
    let mut pos = from;
    for _ in 0..SURFACE_PROBE {
        pos = pos.translated(0, 1);
        if exclude.contains(&pos) || entities.iter().any(|entity| entity.contains_cell(pos)) {
            continue;
        }
        match world
            .get_cell(pos)
            .map(|cell| content::phase(cell.material))
        {
            Some(Phase::Empty) if !claimed.contains(&pos) => return Some(pos),
            Some(Phase::Empty | Phase::Liquid | Phase::Gas) => {}
            _ => return None,
        }
    }
    None
}
