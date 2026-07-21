mod contact;
mod island;
mod rotation;
mod step;

pub use island::detect_island;
use step::{SETTLE_SECS, settle_body, settle_body_quiet};

use crate::physics::ActorAabb;
use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{
    CARDINAL_NEIGHBORS as NEIGHBORS, Cell, CellPos, MaterialId, Phase, Subcell,
};
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

    fn clear(&mut self) {
        self.cells.clear();
        self.set.clear();
    }
}

#[derive(Debug, Clone, Default)]
struct OwnerMap {
    owner: FxHashMap<CellPos, usize>,
}

impl OwnerMap {
    fn rebuild(&mut self, bodies: &[PixelBody]) {
        self.owner.clear();
        for (index, body) in bodies.iter().enumerate() {
            for &pos in &body.raster.set {
                self.owner.insert(pos, index);
            }
        }
    }

    fn get(&self, pos: CellPos) -> Option<usize> {
        self.owner.get(&pos).copied()
    }

    fn insert(&mut self, index: usize, body: &PixelBody) {
        for &pos in &body.raster.set {
            self.owner.insert(pos, index);
        }
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

#[derive(Default)]
pub struct BodySet {
    bodies: Vec<PixelBody>,
    owners: OwnerMap,
    next_id: u32,
    stepper: step::BodyStepper,
}

impl BodySet {
    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    fn body_at_mut(&mut self, pos: CellPos) -> Option<&mut PixelBody> {
        self.owners
            .get(pos)
            .and_then(|index| self.bodies.get_mut(index))
    }

    pub fn receive_player_contact(&mut self, pos: CellPos, wake: bool) -> Option<bool> {
        let body = self.body_at_mut(pos)?;
        if body.frozen {
            return Some(false);
        }
        if wake {
            body.rest_secs = 0.0;
        }
        Some(true)
    }

    pub fn wake_at(&mut self, pos: CellPos) {
        wake_covering(&mut self.bodies, &self.owners, pos);
    }

    pub fn register_island(&mut self, world: &mut CellWorld, island: &[CellPos]) {
        let id = self.allocate_id();
        self.bodies.push(island::register_body(world, id, island));
        let index = self.bodies.len() - 1;
        self.owners.insert(index, &self.bodies[index]);
    }

    pub fn apply_damage(&mut self, world: &mut CellWorld, notes: &mut Vec<CellPos>) {
        if notes.is_empty() {
            return;
        }
        let next_id = &mut self.next_id;
        island::apply_damage(world, &mut self.bodies, &self.owners, notes, || {
            let id = *next_id;
            *next_id = next_id.checked_add(1).expect("body id space exhausted");
            id
        });
        self.owners.rebuild(&self.bodies);
    }

    pub fn step<S>(
        &mut self,
        world: &mut CellWorld,
        entities: &[ActorDynamics],
        gravity: Subcell,
        simulated: &S,
    ) -> &[(f32, f32)]
    where
        S: Fn(fallingsand_core::ChunkPos) -> bool,
    {
        self.stepper.step(
            world,
            &mut self.bodies,
            &mut self.owners,
            entities,
            gravity,
            simulated,
        )
    }

    pub fn settle_resting(&mut self, world: &mut CellWorld) {
        self.settle_where(world, false, |body| {
            !body.frozen && body.rest_secs >= SETTLE_SECS
        });
    }

    pub fn settle_quiet_where(
        &mut self,
        world: &mut CellWorld,
        predicate: impl FnMut(&PixelBody) -> bool,
    ) {
        self.settle_where(world, true, predicate);
    }

    fn settle_where(
        &mut self,
        world: &mut CellWorld,
        quiet: bool,
        mut predicate: impl FnMut(&PixelBody) -> bool,
    ) {
        let mut index = 0;
        let mut changed = false;
        while index < self.bodies.len() {
            if predicate(&self.bodies[index]) {
                let body = self.bodies.swap_remove(index);
                if quiet {
                    settle_body_quiet(world, &body);
                } else {
                    settle_body(world, &body);
                }
                changed = true;
            } else {
                index += 1;
            }
        }
        if changed {
            self.owners.rebuild(&self.bodies);
        }
    }

    fn allocate_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("body id space exhausted");
        id
    }
}

#[derive(Debug, Clone)]
pub struct PixelBody {
    id: u32,
    width: u8,
    height: u8,
    cells: Vec<Cell>,
    perimeter: Vec<(u8, u8)>,
    com_local: (f32, f32),
    pivot: (i32, i32),
    angle_steps: u32,
    x: Subcell,
    y: Subcell,
    vx: Subcell,
    vy: Subcell,
    angle: f32,
    spin: f32,
    inv_mass: f32,
    inv_inertia: f32,
    restitution: f32,
    rest_secs: f32,
    raster: Raster,
    frozen: bool,
}

fn wake_covering(bodies: &mut [PixelBody], owners: &OwnerMap, pos: CellPos) {
    if let Some(body) = owners.get(pos).and_then(|index| bodies.get_mut(index)) {
        body.rest_secs = 0.0;
    }
}

fn append_vacated_wake_targets(
    targets: &mut Vec<CellPos>,
    world: &CellWorld,
    covers: &dyn Fn(CellPos) -> bool,
    vacated: &[CellPos],
) {
    targets.clear();
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
}

impl PixelBody {
    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    pub fn center_cell(&self) -> CellPos {
        CellPos::new(self.x.floor_cell(), self.y.floor_cell())
    }

    fn offset_with(&self, sin: f32, cos: f32, lx: f32, ly: f32) -> (f32, f32) {
        let (dx, dy) = (lx - self.com_local.0, ly - self.com_local.1);
        (dx * cos - dy * sin, dx * sin + dy * cos)
    }

    fn local_offset(&self, lx: f32, ly: f32) -> (f32, f32) {
        let (sin, cos) = quantized_trig_of(self.angle, self.angle_steps);
        self.offset_with(sin, cos, lx, ly)
    }

    fn pivot_cell(&self, x: Subcell, y: Subcell) -> CellPos {
        let (px, py) = self.pivot;
        let ox = px as f32 + 0.5 - self.com_local.0;
        let oy = py as f32 + 0.5 - self.com_local.1;
        CellPos::new(x.add_cells(ox).floor_cell(), y.add_cells(oy).floor_cell())
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

fn rasterize_at(body: &PixelBody, x: Subcell, y: Subcell, angle: f32) -> Raster {
    let mut raster = Raster::default();
    rasterize_into(&mut raster, body, x, y, angle);
    raster
}

fn rasterize_into(raster: &mut Raster, body: &PixelBody, x: Subcell, y: Subcell, angle: f32) {
    raster.clear();
    let step = quantize_step(angle, body.angle_steps);
    let pivot_cell = body.pivot_cell(x, y);
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
