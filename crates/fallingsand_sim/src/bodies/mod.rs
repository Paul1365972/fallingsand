mod contact;
mod island;
mod rotation;
mod solver;
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
use rustc_hash::FxHashSet;
use std::time::Instant;

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

#[derive(Debug, Clone, Copy)]
pub struct BodyPose {
    pub pivot: CellPos,
    pub x: Subcell,
    pub y: Subcell,
    pub angle: f32,
    pub angle_steps: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Raster {
    pub(crate) cells: Vec<(CellPos, u16)>,
    pub(crate) set: FxHashSet<CellPos>,
    pub(crate) pivot: Option<CellPos>,
}

impl Raster {
    pub(crate) fn covers(&self, pos: CellPos) -> bool {
        self.set.contains(&pos)
    }

    fn clear(&mut self) {
        self.cells.clear();
        self.set.clear();
        self.pivot = None;
    }
}

#[derive(Debug, Clone)]
struct BodyCell {
    cell: Cell,
    local: (i32, i32),
    mass: f32,
}

#[derive(Default)]
pub struct BodySet {
    bodies: Vec<PixelBody>,
    next_id: u32,
    stepper: step::BodyStepper,
}

#[derive(Debug, Clone, Copy)]
pub struct BodyCounts {
    pub live: usize,
    pub members: usize,
    pub awake: usize,
    pub resting: usize,
    pub frozen: usize,
}

impl BodySet {
    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    pub fn debug_cells_in(&self, chunk: fallingsand_core::ChunkPos) -> Vec<(u32, CellPos)> {
        self.bodies
            .iter()
            .flat_map(|body| {
                body.raster
                    .cells
                    .iter()
                    .filter(move |(pos, _)| pos.chunk() == chunk)
                    .map(move |&(pos, _)| (body.id, pos))
            })
            .collect()
    }

    pub fn counts(&self) -> BodyCounts {
        let mut counts = BodyCounts {
            live: self.bodies.len(),
            members: 0,
            awake: 0,
            resting: 0,
            frozen: 0,
        };
        for body in &self.bodies {
            counts.members += body.raster.cells.len();
            if body.frozen {
                counts.frozen += 1;
            } else if body.rest_secs > 0.0 {
                counts.resting += 1;
            } else {
                counts.awake += 1;
            }
        }
        counts
    }

    fn body_index_at(&self, pos: CellPos) -> Option<usize> {
        self.bodies.iter().position(|body| body.covers(pos))
    }

    fn body_at_mut(&mut self, pos: CellPos) -> Option<&mut PixelBody> {
        let index = self.body_index_at(pos)?;
        self.bodies.get_mut(index)
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

    pub fn wake_many(&mut self, positions: &[CellPos]) {
        if positions.is_empty() {
            return;
        }
        let positions: FxHashSet<_> = positions.iter().copied().collect();
        for body in &mut self.bodies {
            if body
                .raster
                .cells
                .iter()
                .any(|(pos, _)| positions.contains(pos))
            {
                body.rest_secs = 0.0;
            }
        }
    }

    pub fn register_island(&mut self, world: &mut CellWorld, island: &[CellPos]) {
        let id = self.allocate_id();
        self.bodies
            .push(island::register_body(world, id, island, 0.0, None));
    }

    pub fn register_island_with_pose(
        &mut self,
        world: &mut CellWorld,
        island: &[CellPos],
        pose: BodyPose,
    ) {
        let id = self.allocate_id();
        let mut body = island::register_body(world, id, island, pose.angle, None);
        if body.covers(pose.pivot) {
            body.pivot = pose.pivot;
        }
        body.x = pose.x;
        body.y = pose.y;
        if matches!(pose.angle_steps, ANGLE_STEPS | ANGLE_STEPS_LARGE) {
            body.angle_steps = pose.angle_steps;
        }
        island::derive_body(world, &mut body);
        self.bodies.push(body);
    }

    pub fn poses(&self) -> Vec<BodyPose> {
        self.bodies.iter().map(PixelBody::pose).collect()
    }

    pub fn apply_damage(&mut self, world: &mut CellWorld, notes: &mut Vec<CellPos>) {
        if notes.is_empty() {
            return;
        }
        let next_id = &mut self.next_id;
        island::apply_damage(world, &mut self.bodies, notes, || {
            let id = *next_id;
            *next_id = next_id.checked_add(1).expect("body id space exhausted");
            id
        });
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
        let bodies_before = self.bodies.len();
        let reflood_start = Instant::now();
        let (flooded_bodies, split_bodies, split_fragments) = self.reflood_awake(world);
        let reflood_us = reflood_start.elapsed().as_micros() as u64;
        let bodies_after_reflood = self.bodies.len();
        let derive_start = Instant::now();
        let mut index = 0;
        while index < self.bodies.len() {
            if self.bodies[index].rest_secs > 0.0 {
                index += 1;
                continue;
            }
            if island::derive_body(world, &mut self.bodies[index]) {
                index += 1;
            } else {
                let body = self.bodies.swap_remove(index);
                release_unowned_cells(world, &body, &self.bodies);
            }
        }
        let derive_us = derive_start.elapsed().as_micros() as u64;
        let bodies_after_derive = self.bodies.len();
        self.stepper.step(
            world,
            &mut self.bodies,
            entities,
            gravity,
            simulated,
            step::PreparationDiagnostics {
                reflood_us,
                derive_us,
                bodies_before,
                bodies_after_reflood,
                bodies_after_derive,
                flooded_bodies,
                split_bodies,
                split_fragments,
            },
        )
    }

    fn reflood_awake(&mut self, world: &mut CellWorld) -> (usize, usize, usize) {
        let mut index = 0;
        let mut flooded_bodies = 0;
        let mut split_bodies = 0;
        let mut split_fragments = 0;
        while index < self.bodies.len() {
            if self.bodies[index].rest_secs > 0.0 {
                index += 1;
                continue;
            }
            flooded_bodies += 1;
            let components = island::split_components(world, &self.bodies[index]);
            let intact = components.len() == 1
                && components[0].len() == self.bodies[index].raster.cells.len();
            if intact {
                index += 1;
                continue;
            }
            split_bodies += 1;
            split_fragments += components.len();
            let body = self.bodies.swap_remove(index);
            let offset = island::pose_offset(world, &body);
            let inherited = island::inherited_component(&components, body.pivot);
            for (component_index, component) in components.into_iter().enumerate() {
                let inherits = component_index == inherited;
                let id = if inherits {
                    body.id
                } else {
                    self.allocate_id()
                };
                self.bodies.push(island::register_body(
                    world,
                    id,
                    &component,
                    body.angle,
                    if inherits { offset } else { None },
                ));
            }
        }
        (flooded_bodies, split_bodies, split_fragments)
    }

    pub fn settle_resting(&mut self, world: &mut CellWorld) -> Vec<BodyPose> {
        self.settle_where(world, false, |body| {
            !body.frozen && body.rest_secs >= SETTLE_SECS
        })
    }

    pub fn settle_quiet_where(
        &mut self,
        world: &mut CellWorld,
        predicate: impl FnMut(&PixelBody) -> bool,
    ) -> Vec<BodyPose> {
        self.settle_where(world, true, predicate)
    }

    fn settle_where(
        &mut self,
        world: &mut CellWorld,
        quiet: bool,
        mut predicate: impl FnMut(&PixelBody) -> bool,
    ) -> Vec<BodyPose> {
        let mut index = 0;
        let mut poses = Vec::new();
        while index < self.bodies.len() {
            if predicate(&self.bodies[index]) {
                let body = self.bodies.swap_remove(index);
                poses.push(body.pose());
                if quiet || body.liquid_resting {
                    settle_body_quiet(world, &body);
                } else {
                    settle_body(world, &body);
                }
            } else {
                index += 1;
            }
        }
        poses
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

fn release_unowned_cells(world: &mut CellWorld, body: &PixelBody, owners: &[PixelBody]) {
    for &(pos, _) in &body.raster.cells {
        if owners.iter().any(|owner| owner.covers(pos)) {
            continue;
        }
        if let Some(mut cell) = world.get_cell(pos).filter(|cell| cell.is_body()) {
            cell.set_body(false);
            world.set_cell_raw(pos, cell);
        }
    }
}

#[derive(Debug, Clone)]
pub struct PixelBody {
    id: u32,
    width: u8,
    height: u8,
    pivot: CellPos,
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
    liquid_resting: bool,
    raster: Raster,
    cells: Vec<BodyCell>,
    perimeter: Vec<CellPos>,
    frozen: bool,
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

    pub fn covers(&self, pos: CellPos) -> bool {
        self.raster.covers(pos)
    }

    fn pose(&self) -> BodyPose {
        BodyPose {
            pivot: self.pivot,
            x: self.x,
            y: self.y,
            angle: self.angle,
            angle_steps: self.angle_steps,
        }
    }

    fn transformed_com(&self, step: u32) -> (f32, f32) {
        let mut mass = 0.0;
        let mut sum = (0.0, 0.0);
        for body_cell in &self.cells {
            let (dx, dy) =
                rotate_offset(step, self.angle_steps, body_cell.local.0, body_cell.local.1);
            mass += body_cell.mass;
            sum.0 += body_cell.mass * dx as f32;
            sum.1 += body_cell.mass * dy as f32;
        }
        if mass == 0.0 {
            (0.0, 0.0)
        } else {
            (sum.0 / mass, sum.1 / mass)
        }
    }

    fn pivot_cell_at(&self, x: Subcell, y: Subcell, step: u32) -> CellPos {
        let com = self.transformed_com(step);
        CellPos::new(
            x.add_cells(-com.0).floor_cell(),
            y.add_cells(-com.1).floor_cell(),
        )
    }
}

fn angle_steps_for(width: u8, height: u8) -> u32 {
    if width.max(height) as i32 >= LARGE_BODY_EXTENT {
        ANGLE_STEPS_LARGE
    } else {
        ANGLE_STEPS
    }
}

fn rasterize_into(raster: &mut Raster, body: &PixelBody, x: Subcell, y: Subcell, angle: f32) {
    raster.clear();
    let step = quantize_step(angle, body.angle_steps);
    let pivot = body.pivot_cell_at(x, y, step);
    raster.pivot = Some(pivot);
    for (index, body_cell) in body.cells.iter().enumerate() {
        let (dx, dy) = rotate_offset(step, body.angle_steps, body_cell.local.0, body_cell.local.1);
        let pos = pivot.translated(dx, dy);
        if raster.set.insert(pos) {
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
    displaced.sort_unstable_by_key(|&(pos, cell)| {
        (
            std::cmp::Reverse(content::density_milli(cell.material)),
            pos.y,
            pos.x,
        )
    });

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
        if world.get_cell(pos) != Some(cell) {
            world.set_cell_raw(pos, cell);
        }
    }
    for &(pos, local) in &new.cells {
        let cell = cell_for(local);
        if world.get_cell(pos) != Some(cell) {
            if old.covers(pos) {
                world.set_cell_raw_quiet(pos, cell);
            } else {
                world.set_cell_raw(pos, cell);
            }
        }
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
        ring.sort_by_key(|&(dx, dy)| (-dy, dx.abs(), dx));
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
