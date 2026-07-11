mod contact;
mod island;
mod step;

pub use island::{apply_damage, detect_island, register_body};
pub use step::{settle_body, step_bodies};

use crate::physics::ActorAabb;
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, Fixed, MaterialId, MaterialRegistry, Phase};
use rustc_hash::FxHashSet;

const ANGLE_STEPS: u32 = 1024;
const REFERENCE_DENSITY: f32 = 1000.0;
const RELOCATE_RADIUS: i32 = 8;
const SURFACE_PROBE: i32 = 256;

fn cell_mass(registry: &MaterialRegistry, material: MaterialId) -> f32 {
    registry.get(material).density / REFERENCE_DENSITY
}

fn quantized_trig(angle: f32) -> (f32, f32) {
    const STEP: f32 = std::f32::consts::TAU / ANGLE_STEPS as f32;
    let k = (angle / STEP).round() as i64;
    let k = k.rem_euclid(ANGLE_STEPS as i64);
    (k as f32 * STEP).sin_cos()
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

#[derive(Debug, Clone)]
pub struct PixelBody {
    pub id: u32,
    pub(crate) width: u8,
    pub(crate) height: u8,
    pub(crate) cells: Vec<Cell>,
    pub(crate) perimeter: Vec<(u8, u8)>,
    pub(crate) com_local: (f32, f32),
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
    pub asleep: bool,
}

pub fn wake_covering(bodies: &mut [PixelBody], pos: CellPos) {
    for body in bodies.iter_mut() {
        if body.raster.covers(pos) {
            body.asleep = false;
            body.rest_secs = 0.0;
            return;
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

    fn world_cell_with(&self, sin: f32, cos: f32, x: Fixed, y: Fixed, lx: f32, ly: f32) -> CellPos {
        let (ox, oy) = self.offset_with(sin, cos, lx, ly);
        CellPos::new(x.add_f32(ox).floor_cell(), y.add_f32(oy).floor_cell())
    }

    fn local_offset(&self, lx: f32, ly: f32) -> (f32, f32) {
        let (sin, cos) = quantized_trig(self.angle);
        self.offset_with(sin, cos, lx, ly)
    }
}

fn rasterize_at(body: &PixelBody, x: Fixed, y: Fixed, angle: f32) -> Raster {
    let (sin, cos) = quantized_trig(angle);
    let mut raster = Raster::default();
    for ly in 0..body.height {
        for lx in 0..body.width {
            let index = ly as usize * body.width as usize + lx as usize;
            if body.cells[index].is_air() {
                continue;
            }
            let pos = body.world_cell_with(sin, cos, x, y, lx as f32 + 0.5, ly as f32 + 0.5);
            if raster.set.insert(pos) {
                raster.cells.push((pos, index as u16));
            }
        }
    }
    raster
}

pub(crate) fn commit_stamp(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
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
        match registry.get(cell.material).phase {
            Phase::Solid | Phase::Powder => return None,
            Phase::Empty => {}
            Phase::Liquid | Phase::Gas | Phase::Fire => displaced.push((pos, cell)),
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
        let spot = relocation_spot(world, registry, entities, &claimed, &new.set, from)?;
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
    registry: &MaterialRegistry,
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
                .is_some_and(|cell| registry.get(cell.material).phase == Phase::Empty);
            if empty {
                return Some(pos);
            }
        }
    }
    surface_spot(world, registry, entities, claimed, exclude, from)
}

fn surface_spot(
    world: &CellWorld,
    registry: &MaterialRegistry,
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
            .map(|cell| registry.get(cell.material).phase)
        {
            Some(Phase::Empty) if !claimed.contains(&pos) => return Some(pos),
            Some(Phase::Empty | Phase::Liquid | Phase::Gas | Phase::Fire) => {}
            _ => return None,
        }
    }
    None
}
