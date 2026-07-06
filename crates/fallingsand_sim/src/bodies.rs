use crate::obstacles::EntityBox;
use crate::physics::{FLUID_DRAG_LINEAR, FLUID_DRAG_QUAD, MAX_FLUID_DRAG};
use crate::world::CellWorld;
use fallingsand_core::{
    Cell, CellPos, ChunkPos, Fixed, MaterialId, MaterialRegistry, Phase, TICK_DT,
};
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

pub const MAX_ISLAND_EXTENT: i32 = 48;
pub const MAX_ISLAND_CELLS: usize = 2048;
pub const SLEEP_SECS: f32 = 0.33;
pub const ANGLE_STEPS: u32 = 1024;
const WAKE_SPEED: f32 = 0.5;
const SETTLE_SPEED_SQ: f32 = 100.0;
const SETTLE_SPIN: f32 = 1.5;
const SUPPORT_NORMAL_Y: f32 = 0.25;
const CONTACT_KEEP_PER_SEC: f32 = 0.25;
const BLOCKED_DAMPING: f32 = 0.5;
const BOUNCE_MIN_SPEED: f32 = 30.0;
const FRICTION: f32 = 0.4;
const CONTACT_ITERATIONS: usize = 4;
const PENETRATION_CORRECTION: f32 = 0.5;
const SUBSTEP_TRAVEL: f32 = 0.5;
const REFERENCE_DENSITY: f32 = 1000.0;
const RELOCATE_RADIUS: i32 = 8;
const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

pub fn cell_mass(registry: &MaterialRegistry, material: MaterialId) -> f32 {
    registry.get(material).density / REFERENCE_DENSITY
}

fn quantized_trig(angle: f32) -> (f32, f32) {
    const STEP: f32 = std::f32::consts::TAU / ANGLE_STEPS as f32;
    let k = (angle / STEP).round() as i64;
    let k = k.rem_euclid(ANGLE_STEPS as i64);
    (k as f32 * STEP).sin_cos()
}

#[derive(Debug, Clone, Copy)]
pub struct EntityDynamics {
    pub bbox: EntityBox,
    pub vx: f32,
    pub vy: f32,
    pub inv_mass: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Raster {
    pub cells: Vec<(CellPos, u16)>,
    pub set: FxHashSet<CellPos>,
}

impl Raster {
    pub fn covers(&self, pos: CellPos) -> bool {
        self.set.contains(&pos)
    }
}

#[derive(Debug, Clone)]
pub struct PixelBody {
    pub id: u32,
    pub width: u8,
    pub height: u8,
    pub cells: Vec<Cell>,
    pub perimeter: Vec<(u8, u8)>,
    pub com_local: (f32, f32),
    pub x: Fixed,
    pub y: Fixed,
    pub vx: Fixed,
    pub vy: Fixed,
    pub angle: f32,
    pub spin: f32,
    pub inv_mass: f32,
    pub inv_inertia: f32,
    pub restitution: f32,
    pub rest_secs: f32,
    pub raster: Raster,
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
    pub fn cell_at(&self, x: u8, y: u8) -> Cell {
        self.cells[y as usize * self.width as usize + x as usize]
    }

    fn offset_with(&self, sin: f32, cos: f32, lx: f32, ly: f32) -> (f32, f32) {
        let (dx, dy) = (lx - self.com_local.0, ly - self.com_local.1);
        (dx * cos - dy * sin, dx * sin + dy * cos)
    }

    fn world_cell_with(&self, sin: f32, cos: f32, x: Fixed, y: Fixed, lx: f32, ly: f32) -> CellPos {
        let (ox, oy) = self.offset_with(sin, cos, lx, ly);
        CellPos::new(x.add_f32(ox).floor_cell(), y.add_f32(oy).floor_cell())
    }

    pub fn local_offset(&self, lx: f32, ly: f32) -> (f32, f32) {
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

struct Shape {
    mass: f32,
    com: (f32, f32),
    inertia: f32,
    restitution: f32,
    perimeter: Vec<(u8, u8)>,
}

fn derive_shape(
    registry: &MaterialRegistry,
    cells: &[Cell],
    width: u8,
    height: u8,
) -> Option<Shape> {
    let mut mass = 0.0f32;
    let mut com = (0.0f32, 0.0f32);
    let mut restitution = 0.0f32;
    for ly in 0..height {
        for lx in 0..width {
            let cell = cells[ly as usize * width as usize + lx as usize];
            if cell.is_air() {
                continue;
            }
            let m = cell_mass(registry, cell.material);
            mass += m;
            com.0 += m * (lx as f32 + 0.5);
            com.1 += m * (ly as f32 + 0.5);
            restitution += m * registry.get(cell.material).restitution;
        }
    }
    if mass <= 0.0 {
        return None;
    }
    com.0 /= mass;
    com.1 /= mass;
    restitution /= mass;

    let mut inertia = 0.0f32;
    let mut perimeter = Vec::new();
    for ly in 0..height {
        for lx in 0..width {
            let cell = cells[ly as usize * width as usize + lx as usize];
            if cell.is_air() {
                continue;
            }
            let m = cell_mass(registry, cell.material);
            let (dx, dy) = (lx as f32 + 0.5 - com.0, ly as f32 + 0.5 - com.1);
            inertia += m * (dx * dx + dy * dy + 1.0 / 6.0);
            let edge = [(1i16, 0i16), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|&(ox, oy)| {
                    let (nx, ny) = (lx as i16 + ox, ly as i16 + oy);
                    nx < 0
                        || ny < 0
                        || nx >= width as i16
                        || ny >= height as i16
                        || cells[ny as usize * width as usize + nx as usize].is_air()
                });
            if edge {
                perimeter.push((lx, ly));
            }
        }
    }
    Some(Shape {
        mass,
        com,
        inertia,
        restitution,
        perimeter,
    })
}

fn is_rigid(world: &CellWorld, registry: &MaterialRegistry, pos: CellPos) -> Option<Cell> {
    let cell = world.get_cell(pos)?;
    if cell.is_body() {
        return None;
    }
    let material = registry.get(cell.material);
    (material.phase == Phase::Solid && material.rigid_capable).then_some(cell)
}

pub fn detect_island(
    world: &CellWorld,
    registry: &MaterialRegistry,
    seed: CellPos,
) -> Option<Vec<CellPos>> {
    is_rigid(world, registry, seed)?;
    let mut visited: FxHashSet<CellPos> = FxHashSet::default();
    let mut queue: VecDeque<CellPos> = VecDeque::new();
    visited.insert(seed);
    queue.push_back(seed);
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (seed.x, seed.x, seed.y, seed.y);

    while let Some(pos) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = pos.translated(dx, dy);
            if visited.contains(&next) || is_rigid(world, registry, next).is_none() {
                continue;
            }
            min_x = min_x.min(next.x);
            max_x = max_x.max(next.x);
            min_y = min_y.min(next.y);
            max_y = max_y.max(next.y);
            if max_x - min_x >= MAX_ISLAND_EXTENT
                || max_y - min_y >= MAX_ISLAND_EXTENT
                || visited.len() >= MAX_ISLAND_CELLS
            {
                return None;
            }
            visited.insert(next);
            queue.push_back(next);
        }
    }
    Some(visited.into_iter().collect())
}

pub fn register_body(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    id: u32,
    island: &[CellPos],
) -> PixelBody {
    let min_x = island.iter().map(|p| p.x).min().unwrap();
    let max_x = island.iter().map(|p| p.x).max().unwrap();
    let min_y = island.iter().map(|p| p.y).min().unwrap();
    let max_y = island.iter().map(|p| p.y).max().unwrap();
    let width = (max_x - min_x + 1) as u8;
    let height = (max_y - min_y + 1) as u8;

    let mut cells = vec![Cell::AIR; width as usize * height as usize];
    for pos in island {
        let mut cell = world.get_cell(*pos).unwrap();
        cell.set_body(false);
        let (lx, ly) = ((pos.x - min_x) as usize, (pos.y - min_y) as usize);
        cells[ly * width as usize + lx] = cell;
    }

    let shape = derive_shape(registry, &cells, width, height).expect("island has cells");
    let mut body = PixelBody {
        id,
        width,
        height,
        cells,
        perimeter: shape.perimeter,
        com_local: shape.com,
        x: Fixed::from_cell(min_x).add_f32(shape.com.0),
        y: Fixed::from_cell(min_y).add_f32(shape.com.1),
        vx: Fixed::ZERO,
        vy: Fixed::ZERO,
        angle: 0.0,
        spin: 0.0,
        inv_mass: 1.0 / shape.mass,
        inv_inertia: 1.0 / shape.inertia,
        restitution: shape.restitution,
        rest_secs: 0.0,
        raster: Raster::default(),
        frozen: false,
        asleep: false,
    };
    body.raster = rasterize_at(&body, body.x, body.y, body.angle);
    debug_assert_eq!(body.raster.cells.len(), island.len());
    for &(pos, local) in &body.raster.cells {
        let mut cell = body.cells[local as usize];
        cell.set_body(true);
        world.set_cell_raw(pos, cell);
    }
    body
}

pub fn apply_damage(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    bodies: &mut Vec<PixelBody>,
    mut notes: Vec<CellPos>,
    mut next_id: impl FnMut() -> u32,
) {
    notes.sort_unstable_by_key(|pos| (pos.y, pos.x));
    notes.dedup();
    let mut touched: FxHashSet<usize> = FxHashSet::default();
    for pos in notes {
        let Some(index) = bodies.iter().position(|body| body.raster.covers(pos)) else {
            continue;
        };
        let body = &mut bodies[index];
        let entry = body
            .raster
            .cells
            .iter()
            .position(|&(p, _)| p == pos)
            .expect("raster set matches entries");
        let local = body.raster.cells[entry].1 as usize;
        let world_cell = world.get_cell(pos).unwrap_or(Cell::AIR);
        let adopt = !world_cell.is_air()
            && !world_cell.is_body()
            && registry.get(world_cell.material).phase == Phase::Solid;
        if adopt {
            body.cells[local] = world_cell;
            let mut flagged = world_cell;
            flagged.set_body(true);
            world.set_cell_raw(pos, flagged);
        } else {
            body.cells[local] = Cell::AIR;
            body.raster.cells.remove(entry);
            body.raster.set.remove(&pos);
        }
        touched.insert(index);
    }

    let mut touched: Vec<usize> = touched.into_iter().collect();
    touched.sort_unstable_by(|a, b| b.cmp(a));
    for index in touched {
        let body = bodies.swap_remove(index);
        let parts = split_body(world, registry, body, &mut next_id);
        bodies.extend(parts);
    }
}

fn split_body(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    body: PixelBody,
    mut next_id: impl FnMut() -> u32,
) -> Vec<PixelBody> {
    let width = body.width as usize;
    let mut component: Vec<u16> = vec![0; body.cells.len()];
    let mut count = 0u16;
    for start in 0..body.cells.len() {
        if body.cells[start].is_air() || component[start] != 0 {
            continue;
        }
        count += 1;
        let mut queue = VecDeque::new();
        component[start] = count;
        queue.push_back(start);
        while let Some(index) = queue.pop_front() {
            let (lx, ly) = (index % width, index / width);
            for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (lx as i32 + dx, ly as i32 + dy);
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= body.height as i32 {
                    continue;
                }
                let neighbor = ny as usize * width + nx as usize;
                if component[neighbor] == 0 && !body.cells[neighbor].is_air() {
                    component[neighbor] = count;
                    queue.push_back(neighbor);
                }
            }
        }
    }

    let mut remap: Vec<Option<(u16, u16)>> = vec![None; body.cells.len()];
    let mut parts: Vec<PixelBody> = Vec::new();
    for part in 1..=count {
        let mut min_x = usize::MAX;
        let mut min_y = usize::MAX;
        let mut max_x = 0usize;
        let mut max_y = 0usize;
        for (index, &owner) in component.iter().enumerate() {
            if owner != part {
                continue;
            }
            let (lx, ly) = (index % width, index / width);
            min_x = min_x.min(lx);
            min_y = min_y.min(ly);
            max_x = max_x.max(lx);
            max_y = max_y.max(ly);
        }
        let part_w = (max_x - min_x + 1) as u8;
        let part_h = (max_y - min_y + 1) as u8;
        let mut cells = vec![Cell::AIR; part_w as usize * part_h as usize];
        for (index, &owner) in component.iter().enumerate() {
            if owner != part {
                continue;
            }
            let (lx, ly) = (index % width, index / width);
            let new_local = (ly - min_y) * part_w as usize + (lx - min_x);
            cells[new_local] = body.cells[index];
            remap[index] = Some((parts.len() as u16, new_local as u16));
        }
        let Some(shape) = derive_shape(registry, &cells, part_w, part_h) else {
            for slot in remap.iter_mut() {
                if let Some((owner, _)) = slot
                    && *owner == parts.len() as u16
                {
                    *slot = None;
                }
            }
            continue;
        };
        let old_local = (min_x as f32 + shape.com.0, min_y as f32 + shape.com.1);
        let (rx, ry) = body.local_offset(old_local.0, old_local.1);
        parts.push(PixelBody {
            id: if part == 1 { body.id } else { next_id() },
            width: part_w,
            height: part_h,
            cells,
            perimeter: shape.perimeter,
            com_local: shape.com,
            x: body.x.add_f32(rx),
            y: body.y.add_f32(ry),
            vx: body.vx.add_f32(-body.spin * ry),
            vy: body.vy.add_f32(body.spin * rx),
            angle: body.angle,
            spin: body.spin,
            inv_mass: 1.0 / shape.mass,
            inv_inertia: 1.0 / shape.inertia,
            restitution: shape.restitution,
            rest_secs: 0.0,
            raster: Raster::default(),
            frozen: false,
            asleep: false,
        });
    }

    for &(pos, local) in &body.raster.cells {
        match remap[local as usize] {
            Some((owner, new_local)) => {
                let part = &mut parts[owner as usize];
                part.raster.cells.push((pos, new_local));
                part.raster.set.insert(pos);
            }
            None => {
                if let Some(mut cell) = world.get_cell(pos) {
                    cell.set_body(false);
                    world.set_cell_raw(pos, cell);
                }
            }
        }
    }
    parts
}

enum Other {
    Terrain,
    Entity {
        index: usize,
        inv_mass: f32,
        vx: f32,
        vy: f32,
    },
    Body {
        index: usize,
        inv_mass: f32,
        inv_inertia: f32,
        vx: f32,
        vy: f32,
        spin: f32,
        rx: f32,
        ry: f32,
        resting: bool,
    },
}

impl Other {
    const fn is_static(&self) -> bool {
        matches!(self, Other::Terrain | Other::Body { resting: true, .. })
    }
}

struct Contact {
    rx: f32,
    ry: f32,
    nx: f32,
    ny: f32,
    depth: f32,
    restitution: f32,
    other: Other,
}

fn obstructed(
    world: &CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityDynamics],
    own: &FxHashSet<CellPos>,
    pos: CellPos,
) -> bool {
    if own.contains(&pos) {
        return false;
    }
    let solid = match world.get_cell(pos) {
        Some(cell) => matches!(
            registry.get(cell.material).phase,
            Phase::Solid | Phase::Powder
        ),
        None => true,
    };
    solid || entities.iter().any(|entity| entity.bbox.contains_cell(pos))
}

fn find_contacts(
    world: &CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityDynamics],
    bodies: &[PixelBody],
    index: usize,
) -> Vec<Contact> {
    let body = &bodies[index];
    let (sin, cos) = quantized_trig(body.angle);
    let mut contacts: Vec<Contact> = Vec::new();
    for &(lx, ly) in &body.perimeter {
        let (ox, oy) = body.offset_with(sin, cos, lx as f32 + 0.5, ly as f32 + 0.5);
        let (wx, wy) = (body.x.add_f32(ox), body.y.add_f32(oy));
        let pos = CellPos::new(wx.floor_cell(), wy.floor_cell());
        if body.raster.covers(pos) {
            continue;
        }

        let mut depth = 0.5;
        let mut surface = 0.0f32;
        let other = match world.get_cell(pos) {
            None => Other::Terrain,
            Some(cell) if cell.is_body() => {
                let owner = bodies
                    .iter()
                    .position(|other| other.id != body.id && other.raster.covers(pos));
                match owner {
                    Some(other_index) => {
                        let other = &bodies[other_index];
                        surface = other.restitution;
                        Other::Body {
                            index: other_index,
                            inv_mass: other.inv_mass,
                            inv_inertia: other.inv_inertia,
                            vx: other.vx.to_f32(),
                            vy: other.vy.to_f32(),
                            spin: other.spin,
                            rx: (wx - other.x).to_f32(),
                            ry: (wy - other.y).to_f32(),
                            resting: other.asleep || other.rest_secs > 0.0,
                        }
                    }
                    None => {
                        surface = registry.get(cell.material).restitution;
                        Other::Terrain
                    }
                }
            }
            Some(cell)
                if matches!(
                    registry.get(cell.material).phase,
                    Phase::Solid | Phase::Powder
                ) =>
            {
                surface = registry.get(cell.material).restitution;
                Other::Terrain
            }
            Some(_) => {
                let Some(entity_index) = entities
                    .iter()
                    .position(|entity| entity.bbox.contains_cell(pos))
                else {
                    continue;
                };
                let entity = &entities[entity_index];
                let depth_x =
                    entity.bbox.half_w.to_f32() + 0.5 - (wx - entity.bbox.x).to_f32().abs();
                let depth_y =
                    entity.bbox.half_h.to_f32() + 0.5 - (wy - entity.bbox.y).to_f32().abs();
                depth = depth_x.min(depth_y).clamp(0.5, 4.0);
                Other::Entity {
                    index: entity_index,
                    inv_mass: entity.inv_mass,
                    vx: entity.vx,
                    vy: entity.vy,
                }
            }
        };

        let mut nx = 0.0f32;
        let mut ny = 0.0f32;
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                if (dx, dy) == (0, 0) {
                    continue;
                }
                if !obstructed(
                    world,
                    registry,
                    entities,
                    &body.raster.set,
                    pos.translated(dx, dy),
                ) {
                    nx += dx as f32;
                    ny += dy as f32;
                }
            }
        }
        let length = (nx * nx + ny * ny).sqrt();
        let (nx, ny) = if length > 1e-3 {
            (nx / length, ny / length)
        } else {
            (0.0, 1.0)
        };
        contacts.push(Contact {
            rx: ox,
            ry: oy,
            nx,
            ny,
            depth,
            restitution: surface,
            other,
        });
    }
    contacts
}

fn span_simulated(
    world: &CellWorld,
    simulated: &dyn Fn(ChunkPos) -> bool,
    body: &PixelBody,
) -> bool {
    let radius = (0.5 * (body.width as f32).hypot(body.height as f32)).ceil() as i32 + 1;
    let (cx, cy) = (body.x.floor_cell(), body.y.floor_cell());
    let min = CellPos::new(cx - radius, cy - radius).chunk();
    let max = CellPos::new(cx + radius, cy + radius).chunk();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let pos = ChunkPos::new(x, y);
            if world.chunk(pos).is_none() || !simulated(pos) {
                return false;
            }
        }
    }
    true
}

pub fn step_bodies(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    bodies: &mut [PixelBody],
    entities: &[EntityDynamics],
    gravity: Fixed,
    simulated: &dyn Fn(ChunkPos) -> bool,
) -> Vec<(f32, f32)> {
    let mut entity_impulses = vec![(0.0, 0.0); entities.len()];
    let entity_boxes: Vec<EntityBox> = entities.iter().map(|entity| entity.bbox).collect();

    let mut order: Vec<usize> = (0..bodies.len()).collect();
    order.sort_unstable_by_key(|&index| {
        (
            bodies[index].y.raw(),
            bodies[index].x.raw(),
            bodies[index].id,
        )
    });

    for &index in &order {
        let frozen = !span_simulated(world, simulated, &bodies[index]);
        bodies[index].frozen = frozen;
        if frozen || bodies[index].asleep {
            continue;
        }
        {
            let body = &mut bodies[index];
            if body.rest_secs > 0.0
                && body.vx == Fixed::ZERO
                && body.vy == Fixed::ZERO
                && body.spin == 0.0
            {
                body.rest_secs += TICK_DT;
                if body.rest_secs >= SLEEP_SECS {
                    body.asleep = true;
                }
                continue;
            }
        }

        let (start_x, start_y, start_angle) = {
            let body = &bodies[index];
            (body.x, body.y, body.angle)
        };
        let substeps = {
            let body = &mut bodies[index];
            apply_buoyancy(world, registry, body, gravity);
            body.vy += gravity.per_tick();

            let radius = 0.5 * (body.width as f32).hypot(body.height as f32);
            let (vx, vy) = (body.vx.to_f32(), body.vy.to_f32());
            let travel = ((vx * vx + vy * vy).sqrt() + body.spin.abs() * radius) * TICK_DT;
            ((travel / SUBSTEP_TRAVEL).ceil() as u32).max(1)
        };
        let damping = CONTACT_KEEP_PER_SEC.powf(TICK_DT / substeps as f32);

        for _ in 0..substeps {
            step_substep(
                world,
                registry,
                bodies,
                entities,
                index,
                damping,
                substeps,
                &mut entity_impulses,
            );
        }
        let vacated = restamp(
            world,
            registry,
            &entity_boxes,
            &mut bodies[index],
            start_x,
            start_y,
            start_angle,
        );
        for pos in vacated {
            for (dx, dy) in NEIGHBORS {
                let neighbor = pos.translated(dx, dy);
                if bodies[index].raster.covers(neighbor) {
                    continue;
                }
                if world.get_cell(neighbor).is_some_and(|cell| cell.is_body()) {
                    wake_covering(bodies, neighbor);
                }
            }
        }
    }
    entity_impulses
}

fn apply_buoyancy(
    world: &CellWorld,
    registry: &MaterialRegistry,
    body: &mut PixelBody,
    gravity: Fixed,
) {
    const BEARING: [(i32, i32); 3] = [(0, -1), (-1, 0), (1, 0)];
    let (sin, cos) = quantized_trig(body.angle);
    let mut density_sum = 0.0f32;
    let mut samples = 0u32;
    let mut wet = 0u32;
    for &(lx, ly) in &body.perimeter {
        let pos = body.world_cell_with(sin, cos, body.x, body.y, lx as f32 + 0.5, ly as f32 + 0.5);
        let mut bearing = false;
        for (dx, dy) in BEARING {
            let neighbor = pos.translated(dx, dy);
            if body.raster.covers(neighbor) {
                continue;
            }
            let Some(cell) = world.get_cell(neighbor) else {
                continue;
            };
            let material = registry.get(cell.material);
            if material.phase == Phase::Liquid {
                density_sum += material.density;
                samples += 1;
                bearing = true;
            }
        }
        wet += bearing as u32;
    }
    if wet == 0 {
        return;
    }

    let count = body.cells.iter().filter(|cell| !cell.is_air()).count();
    let submersion = wet as f32 / body.perimeter.len().max(1) as f32;
    let buoyant = submersion * count as f32 * (density_sum / samples as f32) / REFERENCE_DENSITY;
    body.vy = body
        .vy
        .add_f32(-gravity.to_f32() * buoyant * body.inv_mass * TICK_DT);
    let speed = body.vx.to_f32().hypot(body.vy.to_f32());
    let drag =
        ((FLUID_DRAG_LINEAR + FLUID_DRAG_QUAD * speed) * submersion * TICK_DT).min(MAX_FLUID_DRAG);
    let keep = Fixed::from_f32(1.0 - drag);
    body.vx = body.vx.mul(keep);
    body.vy = body.vy.mul(keep);
    body.spin *= 1.0 - drag;
}

#[allow(clippy::too_many_arguments)]
fn step_substep(
    world: &CellWorld,
    registry: &MaterialRegistry,
    bodies: &mut [PixelBody],
    entities: &[EntityDynamics],
    index: usize,
    damping: f32,
    substeps: u32,
    entity_impulses: &mut [(f32, f32)],
) {
    let sub_dt = TICK_DT / substeps as f32;
    let (prev_x, prev_y, prev_angle) = {
        let body = &mut bodies[index];
        let prev = (body.x, body.y, body.angle);
        body.x += body.vx.per_substep(substeps);
        body.y += body.vy.per_substep(substeps);
        body.angle = (body.angle + body.spin * sub_dt).rem_euclid(std::f32::consts::TAU);
        prev
    };

    let contacts = find_contacts(world, registry, entities, bodies, index);
    let touching = !contacts.is_empty();
    let static_only = contacts.iter().all(|contact| contact.other.is_static());
    let supported = contacts
        .iter()
        .any(|contact| contact.other.is_static() && contact.ny > SUPPORT_NORMAL_Y);

    let mut body_impulses: Vec<(usize, f32, f32, f32)> = Vec::new();
    {
        let body = &mut bodies[index];
        let (mut vx, mut vy) = (body.vx.to_f32(), body.vy.to_f32());
        for _ in 0..CONTACT_ITERATIONS {
            for contact in &contacts {
                let (other_inv_mass, other_inv_inertia, other_vx, other_vy, r2) =
                    match contact.other {
                        Other::Terrain => (0.0, 0.0, 0.0, 0.0, (0.0, 0.0)),
                        Other::Entity {
                            inv_mass, vx, vy, ..
                        } => (inv_mass, 0.0, vx, vy, (0.0, 0.0)),
                        Other::Body {
                            inv_mass,
                            inv_inertia,
                            vx,
                            vy,
                            spin,
                            rx,
                            ry,
                            ..
                        } => (
                            inv_mass,
                            inv_inertia,
                            vx - spin * ry,
                            vy + spin * rx,
                            (rx, ry),
                        ),
                    };

                let rel_vx = vx - body.spin * contact.ry - other_vx;
                let rel_vy = vy + body.spin * contact.rx - other_vy;
                let vn = rel_vx * contact.nx + rel_vy * contact.ny;
                if vn >= 0.0 {
                    continue;
                }
                let r_cross_n = contact.rx * contact.ny - contact.ry * contact.nx;
                let r2_cross_n = r2.0 * contact.ny - r2.1 * contact.nx;
                let k = body.inv_mass
                    + other_inv_mass
                    + r_cross_n * r_cross_n * body.inv_inertia
                    + r2_cross_n * r2_cross_n * other_inv_inertia;
                let bounce = if -vn > BOUNCE_MIN_SPEED {
                    body.restitution.max(contact.restitution)
                } else {
                    0.0
                };
                let jn = -(1.0 + bounce) * vn / k;
                vx += jn * contact.nx * body.inv_mass;
                vy += jn * contact.ny * body.inv_mass;
                body.spin += r_cross_n * jn * body.inv_inertia;
                apply_to_other(
                    contact,
                    -jn * contact.nx,
                    -jn * contact.ny,
                    entity_impulses,
                    &mut body_impulses,
                );

                let tx = -contact.ny;
                let ty = contact.nx;
                let rel_vx = vx - body.spin * contact.ry - other_vx;
                let rel_vy = vy + body.spin * contact.rx - other_vy;
                let vt = rel_vx * tx + rel_vy * ty;
                let r_cross_t = contact.rx * ty - contact.ry * tx;
                let r2_cross_t = r2.0 * ty - r2.1 * tx;
                let kt = body.inv_mass
                    + other_inv_mass
                    + r_cross_t * r_cross_t * body.inv_inertia
                    + r2_cross_t * r2_cross_t * other_inv_inertia;
                let jt = (-vt / kt).clamp(-FRICTION * jn.abs(), FRICTION * jn.abs());
                vx += jt * tx * body.inv_mass;
                vy += jt * ty * body.inv_mass;
                body.spin += r_cross_t * jt * body.inv_inertia;
                apply_to_other(
                    contact,
                    -jt * tx,
                    -jt * ty,
                    entity_impulses,
                    &mut body_impulses,
                );
            }
        }

        if touching {
            vx *= damping;
            vy *= damping;
            body.spin *= damping;
        }

        let slow = vx * vx + vy * vy < SETTLE_SPEED_SQ && body.spin.abs() < SETTLE_SPIN;
        if touching && slow && static_only && supported {
            body.x = prev_x;
            body.y = prev_y;
            body.angle = prev_angle;
            body.vx = Fixed::ZERO;
            body.vy = Fixed::ZERO;
            body.spin = 0.0;
            body.rest_secs += sub_dt;
        } else {
            let deepest = contacts.iter().max_by(|a, b| {
                a.depth
                    .partial_cmp(&b.depth)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if let Some(deepest) = deepest {
                let correction = (deepest.depth * PENETRATION_CORRECTION).min(1.0);
                body.x = body.x.add_f32(deepest.nx * correction);
                body.y = body.y.add_f32(deepest.ny * correction);
            }
            body.vx = Fixed::from_f32(vx);
            body.vy = Fixed::from_f32(vy);
            body.rest_secs = 0.0;
        }
    }

    for (other_index, jx, jy, r_cross_j) in body_impulses {
        let other = &mut bodies[other_index];
        let dvx = jx * other.inv_mass;
        let dvy = jy * other.inv_mass;
        let dspin = r_cross_j * other.inv_inertia;
        other.vx = other.vx.add_f32(dvx);
        other.vy = other.vy.add_f32(dvy);
        other.spin += dspin;
        if dvx.abs() + dvy.abs() > WAKE_SPEED || dspin.abs() > WAKE_SPEED {
            other.rest_secs = 0.0;
            other.asleep = false;
        }
    }
}

fn apply_to_other(
    contact: &Contact,
    jx: f32,
    jy: f32,
    entity_impulses: &mut [(f32, f32)],
    body_impulses: &mut Vec<(usize, f32, f32, f32)>,
) {
    match contact.other {
        Other::Terrain => {}
        Other::Entity { index, .. } => {
            entity_impulses[index].0 += jx;
            entity_impulses[index].1 += jy;
        }
        Other::Body { index, rx, ry, .. } => {
            body_impulses.push((index, jx, jy, rx * jy - ry * jx));
        }
    }
}

fn restamp(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityBox],
    body: &mut PixelBody,
    start_x: Fixed,
    start_y: Fixed,
    start_angle: f32,
) -> Vec<CellPos> {
    let full = (body.x, body.y, body.angle);
    let candidates = [
        full,
        (body.x, body.y, start_angle),
        (start_x, start_y, body.angle),
    ];
    for (attempt, &(x, y, angle)) in candidates.iter().enumerate() {
        if attempt > 0 && (x, y, angle) == full {
            continue;
        }
        let raster = rasterize_at(body, x, y, angle);
        let committed = if raster.cells == body.raster.cells {
            body.raster = raster;
            Some(Vec::new())
        } else {
            plan_and_commit(world, registry, entities, body, raster)
        };
        if let Some(vacated) = committed {
            body.x = x;
            body.y = y;
            body.angle = angle;
            match attempt {
                1 => body.spin *= BLOCKED_DAMPING,
                2 => {
                    body.vx = body.vx.mul(Fixed::from_f32(BLOCKED_DAMPING));
                    body.vy = body.vy.mul(Fixed::from_f32(BLOCKED_DAMPING));
                }
                _ => {}
            }
            return vacated;
        }
    }

    body.x = start_x;
    body.y = start_y;
    body.angle = start_angle;
    body.vx = body.vx.mul(Fixed::from_f32(BLOCKED_DAMPING));
    body.vy = body.vy.mul(Fixed::from_f32(BLOCKED_DAMPING));
    body.spin *= BLOCKED_DAMPING;
    Vec::new()
}

fn plan_and_commit(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityBox],
    body: &mut PixelBody,
    new: Raster,
) -> Option<Vec<CellPos>> {
    let mut displaced: Vec<(CellPos, Cell)> = Vec::new();
    for &(pos, _) in &new.cells {
        if body.raster.covers(pos) {
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

    let mut vacated: Vec<CellPos> = body
        .raster
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
        let mut cell = body.cells[local as usize];
        cell.set_body(true);
        world.set_cell_raw(pos, cell);
    }
    body.raster = new;
    Some(vacated)
}

pub fn settle_body(world: &mut CellWorld, registry: &MaterialRegistry, body: &PixelBody) {
    let mut winner = vec![false; body.cells.len()];
    for &(_, local) in &body.raster.cells {
        winner[local as usize] = true;
    }
    let (sin, cos) = quantized_trig(body.angle);
    let mut claimed: FxHashSet<CellPos> = FxHashSet::default();
    let mut writes: Vec<(CellPos, Cell)> = Vec::new();
    for ly in 0..body.height {
        for lx in 0..body.width {
            let index = ly as usize * body.width as usize + lx as usize;
            let cell = body.cells[index];
            if cell.is_air() || winner[index] {
                continue;
            }
            let base =
                body.world_cell_with(sin, cos, body.x, body.y, lx as f32 + 0.5, ly as f32 + 0.5);
            let target = [(0, 0), (0, 1), (1, 0), (-1, 0), (0, 2), (0, -1)]
                .iter()
                .map(|&(dx, dy)| base.translated(dx, dy))
                .find(|&pos| {
                    !claimed.contains(&pos)
                        && !body.raster.covers(pos)
                        && world.get_cell(pos).is_some_and(|existing| {
                            !existing.is_body()
                                && !matches!(
                                    registry.get(existing.material).phase,
                                    Phase::Solid | Phase::Powder
                                )
                        })
                })
                .or_else(|| {
                    relocation_spot(world, registry, &[], &claimed, &body.raster.set, base)
                });
            let Some(pos) = target else {
                continue;
            };
            let existing = world.get_cell(pos).expect("settle target is loaded");
            if !existing.is_air() {
                let Some(spot) =
                    relocation_spot(world, registry, &[], &claimed, &body.raster.set, pos)
                else {
                    continue;
                };
                claimed.insert(spot);
                writes.push((spot, existing));
            }
            claimed.insert(pos);
            let mut placed = cell;
            placed.set_body(false);
            writes.push((pos, placed));
        }
    }

    for &(pos, local) in &body.raster.cells {
        let mut cell = body.cells[local as usize];
        cell.set_body(false);
        writes.push((pos, cell));
    }
    for (pos, cell) in writes {
        world.set_cell_raw(pos, cell);
    }
}

fn relocation_spot(
    world: &CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityBox],
    claimed: &FxHashSet<CellPos>,
    exclude: &FxHashSet<CellPos>,
    from: CellPos,
) -> Option<CellPos> {
    for radius in 1..=RELOCATE_RADIUS {
        let mut ring: Vec<(i32, i32)> = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) == radius {
                    ring.push((dx, dy));
                }
            }
        }
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
    None
}
