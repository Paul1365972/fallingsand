use crate::obstacles::{EntityBox, Obstacles};
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, MaterialId, MaterialRegistry, Phase};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

pub const MAX_ISLAND_EXTENT: i32 = 48;
pub const MAX_ISLAND_CELLS: usize = 2048;
pub const SETTLE_SECS: f32 = 0.33;
const SETTLE_SPEED_SQ: f32 = 100.0;
const SETTLE_SPIN: f32 = 1.5;
const CONTACT_DAMPING: f32 = 0.94;
const RESTITUTION: f32 = 0.0;
const FRICTION: f32 = 0.4;
const CONTACT_ITERATIONS: usize = 4;
const PENETRATION_CORRECTION: f32 = 0.5;
const SUBSTEP_TRAVEL: f32 = 0.5;
const FLUID_DRAG: f32 = 2.5;
const REFERENCE_DENSITY: f32 = 1000.0;

pub fn cell_mass(registry: &MaterialRegistry, material: MaterialId) -> f32 {
    registry.get(material).density / REFERENCE_DENSITY
}

#[derive(Debug, Clone, Copy)]
pub struct EntityDynamics {
    pub bbox: EntityBox,
    pub vx: f32,
    pub vy: f32,
    pub inv_mass: f32,
}

#[derive(Debug, Clone)]
pub struct PixelBody {
    pub id: u32,
    pub width: u8,
    pub height: u8,
    pub cells: Vec<Cell>,
    pub perimeter: Vec<(u8, u8)>,
    pub com_local: (f32, f32),
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub angle: f32,
    pub spin: f32,
    pub inv_mass: f32,
    pub inv_inertia: f32,
    pub rest_secs: f32,
}

impl PixelBody {
    pub fn cell_at(&self, x: u8, y: u8) -> Cell {
        self.cells[y as usize * self.width as usize + x as usize]
    }

    pub fn local_to_world(&self, lx: f32, ly: f32) -> (f32, f32) {
        let (dx, dy) = (lx - self.com_local.0, ly - self.com_local.1);
        let (sin, cos) = self.angle.sin_cos();
        (self.x + dx * cos - dy * sin, self.y + dx * sin + dy * cos)
    }
}

struct Shape {
    mass: f32,
    com: (f32, f32),
    inertia: f32,
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
        }
    }
    if mass <= 0.0 {
        return None;
    }
    com.0 /= mass;
    com.1 /= mass;

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
        perimeter,
    })
}

fn is_rigid(world: &CellWorld, registry: &MaterialRegistry, pos: CellPos) -> Option<Cell> {
    let cell = world.get_cell(pos)?;
    let material = registry.get(cell.material);
    (material.phase == Phase::Solid && material.rigid_capable).then_some(cell)
}

fn blocks_body(world: &CellWorld, registry: &MaterialRegistry, pos: CellPos) -> bool {
    match world.get_cell(pos) {
        Some(cell) => matches!(
            registry.get(cell.material).phase,
            Phase::Solid | Phase::Powder
        ),
        None => true,
    }
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

pub fn extract_body(
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
        let cell = world.get_cell(*pos).unwrap();
        let (lx, ly) = ((pos.x - min_x) as usize, (pos.y - min_y) as usize);
        cells[ly * width as usize + lx] = cell;
        world.place_material(*pos, MaterialId::AIR);
    }

    let shape = derive_shape(registry, &cells, width, height).expect("island has cells");
    PixelBody {
        id,
        width,
        height,
        cells,
        perimeter: shape.perimeter,
        com_local: shape.com,
        x: min_x as f32 + shape.com.0,
        y: min_y as f32 + shape.com.1,
        vx: 0.0,
        vy: 0.0,
        angle: 0.0,
        spin: 0.0,
        inv_mass: 1.0 / shape.mass,
        inv_inertia: 1.0 / shape.inertia,
        rest_secs: 0.0,
    }
}

pub fn refresh_body(
    body: &PixelBody,
    registry: &MaterialRegistry,
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
    if count == 0 {
        return Vec::new();
    }

    let mut parts = Vec::new();
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
            cells[(ly - min_y) * part_w as usize + (lx - min_x)] = body.cells[index];
        }
        let Some(shape) = derive_shape(registry, &cells, part_w, part_h) else {
            continue;
        };
        let old_local = (min_x as f32 + shape.com.0, min_y as f32 + shape.com.1);
        let (wx, wy) = body.local_to_world(old_local.0, old_local.1);
        let (rx, ry) = (wx - body.x, wy - body.y);
        parts.push(PixelBody {
            id: if part == 1 { body.id } else { next_id() },
            width: part_w,
            height: part_h,
            cells,
            perimeter: shape.perimeter,
            com_local: shape.com,
            x: wx,
            y: wy,
            vx: body.vx - body.spin * ry,
            vy: body.vy + body.spin * rx,
            angle: body.angle,
            spin: body.spin,
            inv_mass: 1.0 / shape.mass,
            inv_inertia: 1.0 / shape.inertia,
            rest_secs: 0.0,
        });
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
    },
}

struct Contact {
    rx: f32,
    ry: f32,
    nx: f32,
    ny: f32,
    depth: f32,
    other: Other,
}

fn obstructed(
    world: &CellWorld,
    registry: &MaterialRegistry,
    obstacles: &Obstacles,
    entities: &[EntityDynamics],
    self_id: u32,
    pos: CellPos,
) -> bool {
    blocks_body(world, registry, pos)
        || entities.iter().any(|entity| entity.bbox.contains_cell(pos))
        || obstacles.body_at(pos).is_some_and(|(id, _)| id != self_id)
}

fn find_contacts(
    world: &CellWorld,
    registry: &MaterialRegistry,
    obstacles: &Obstacles,
    entities: &[EntityDynamics],
    bodies: &[PixelBody],
    index: usize,
    id_to_index: &FxHashMap<u32, usize>,
) -> Vec<Contact> {
    let body = &bodies[index];
    let mut contacts: Vec<Contact> = Vec::new();
    for &(lx, ly) in &body.perimeter {
        let (wx, wy) = body.local_to_world(lx as f32 + 0.5, ly as f32 + 0.5);
        let pos = CellPos::new(wx.floor() as i32, wy.floor() as i32);

        let mut depth = 0.5;
        let other = if blocks_body(world, registry, pos) {
            Other::Terrain
        } else if let Some(entity_index) = entities
            .iter()
            .position(|entity| entity.bbox.contains_cell(pos))
        {
            let entity = &entities[entity_index];
            let depth_x = entity.bbox.half_w + 0.5 - (wx - entity.bbox.x).abs();
            let depth_y = entity.bbox.half_h + 0.5 - (wy - entity.bbox.y).abs();
            depth = depth_x.min(depth_y).clamp(0.5, 4.0);
            Other::Entity {
                index: entity_index,
                inv_mass: entity.inv_mass,
                vx: entity.vx,
                vy: entity.vy,
            }
        } else if let Some(other_index) = obstacles
            .body_at(pos)
            .filter(|&(id, _)| id != body.id)
            .and_then(|(id, _)| id_to_index.get(&id).copied())
            .filter(|&other_index| other_index != index)
        {
            let other = &bodies[other_index];
            Other::Body {
                index: other_index,
                inv_mass: other.inv_mass,
                inv_inertia: other.inv_inertia,
                vx: other.vx,
                vy: other.vy,
                spin: other.spin,
                rx: wx - other.x,
                ry: wy - other.y,
            }
        } else {
            continue;
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
                    obstacles,
                    entities,
                    body.id,
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
            rx: wx - body.x,
            ry: wy - body.y,
            nx,
            ny,
            depth,
            other,
        });
    }
    contacts
}

pub struct BodiesStep {
    pub settled: Vec<usize>,
    pub entity_impulses: Vec<(f32, f32)>,
}

pub fn step_bodies(
    world: &CellWorld,
    registry: &MaterialRegistry,
    obstacles: &Obstacles,
    bodies: &mut [PixelBody],
    entities: &[EntityDynamics],
    gravity: f32,
    dt: f32,
) -> BodiesStep {
    let mut result = BodiesStep {
        settled: Vec::new(),
        entity_impulses: vec![(0.0, 0.0); entities.len()],
    };
    let id_to_index: FxHashMap<u32, usize> = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| (body.id, index))
        .collect();

    for index in 0..bodies.len() {
        let substeps = {
            let body = &mut bodies[index];

            let mut cell_count = 0.0f32;
            let mut buoyant = 0.0f32;
            let mut wet = 0.0f32;
            for ly in 0..body.height {
                for lx in 0..body.width {
                    let cell = body.cell_at(lx, ly);
                    if cell.is_air() {
                        continue;
                    }
                    cell_count += 1.0;
                    let (wx, wy) = body.local_to_world(lx as f32 + 0.5, ly as f32 + 0.5);
                    let pos = CellPos::new(wx.floor() as i32, wy.floor() as i32);
                    if let Some(world_cell) = world.get_cell(pos) {
                        let material = registry.get(world_cell.material);
                        if material.phase == Phase::Liquid {
                            buoyant += material.density / REFERENCE_DENSITY;
                            wet += 1.0;
                        }
                    }
                }
            }
            if buoyant > 0.0 {
                body.vy -= gravity * buoyant * body.inv_mass * dt;
                let submersion = wet / cell_count.max(1.0);
                let drag = (FLUID_DRAG * submersion * dt).min(0.9);
                body.vx *= 1.0 - drag;
                body.vy *= 1.0 - drag;
                body.spin *= 1.0 - drag;
            }
            body.vy += gravity * dt;

            let radius = 0.5 * (body.width as f32).hypot(body.height as f32);
            let travel =
                ((body.vx * body.vx + body.vy * body.vy).sqrt() + body.spin.abs() * radius) * dt;
            ((travel / SUBSTEP_TRAVEL).ceil() as usize).max(1)
        };
        let sub_dt = dt / substeps as f32;
        let damping = CONTACT_DAMPING.powf(1.0 / substeps as f32);

        for _ in 0..substeps {
            step_substep(
                world,
                registry,
                obstacles,
                bodies,
                entities,
                index,
                &id_to_index,
                damping,
                sub_dt,
                &mut result.entity_impulses,
            );
        }
        if bodies[index].rest_secs >= SETTLE_SECS {
            result.settled.push(index);
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn step_substep(
    world: &CellWorld,
    registry: &MaterialRegistry,
    obstacles: &Obstacles,
    bodies: &mut [PixelBody],
    entities: &[EntityDynamics],
    index: usize,
    id_to_index: &FxHashMap<u32, usize>,
    damping: f32,
    dt: f32,
    entity_impulses: &mut [(f32, f32)],
) {
    {
        let (prev_x, prev_y, prev_angle) = {
            let body = &mut bodies[index];
            let prev = (body.x, body.y, body.angle);
            body.x += body.vx * dt;
            body.y += body.vy * dt;
            body.angle += body.spin * dt;
            prev
        };

        let contacts = find_contacts(
            world,
            registry,
            obstacles,
            entities,
            bodies,
            index,
            id_to_index,
        );
        let touching = !contacts.is_empty();
        let terrain_only = contacts
            .iter()
            .all(|contact| matches!(contact.other, Other::Terrain));

        let mut body_impulses: Vec<(usize, f32, f32, f32)> = Vec::new();
        {
            let body = &mut bodies[index];
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

                    let rel_vx = body.vx - body.spin * contact.ry - other_vx;
                    let rel_vy = body.vy + body.spin * contact.rx - other_vy;
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
                    let jn = -(1.0 + RESTITUTION) * vn / k;
                    body.vx += jn * contact.nx * body.inv_mass;
                    body.vy += jn * contact.ny * body.inv_mass;
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
                    let rel_vx = body.vx - body.spin * contact.ry - other_vx;
                    let rel_vy = body.vy + body.spin * contact.rx - other_vy;
                    let vt = rel_vx * tx + rel_vy * ty;
                    let r_cross_t = contact.rx * ty - contact.ry * tx;
                    let r2_cross_t = r2.0 * ty - r2.1 * tx;
                    let kt = body.inv_mass
                        + other_inv_mass
                        + r_cross_t * r_cross_t * body.inv_inertia
                        + r2_cross_t * r2_cross_t * other_inv_inertia;
                    let jt = (-vt / kt).clamp(-FRICTION * jn.abs(), FRICTION * jn.abs());
                    body.vx += jt * tx * body.inv_mass;
                    body.vy += jt * ty * body.inv_mass;
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
                body.vx *= damping;
                body.vy *= damping;
                body.spin *= damping;
            }

            let slow = body.vx * body.vx + body.vy * body.vy < SETTLE_SPEED_SQ
                && body.spin.abs() < SETTLE_SPIN;
            if touching && slow && terrain_only {
                body.x = prev_x;
                body.y = prev_y;
                body.angle = prev_angle;
                body.vx = 0.0;
                body.vy = 0.0;
                body.spin = 0.0;
                body.rest_secs += dt;
            } else {
                let deepest = contacts.iter().max_by(|a, b| {
                    a.depth
                        .partial_cmp(&b.depth)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                if let Some(deepest) = deepest {
                    let correction = (deepest.depth * PENETRATION_CORRECTION).min(1.0);
                    body.x += deepest.nx * correction;
                    body.y += deepest.ny * correction;
                }
                body.rest_secs = 0.0;
            }
        }

        for (other_index, jx, jy, r_cross_j) in body_impulses {
            let other = &mut bodies[other_index];
            other.vx += jx * other.inv_mass;
            other.vy += jy * other.inv_mass;
            other.spin += r_cross_j * other.inv_inertia;
            other.rest_secs = 0.0;
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

const STAMP_RELOCATE_RADIUS: i32 = 8;

pub fn try_stamp(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityBox],
    body: &PixelBody,
) -> bool {
    let mut writes: Vec<(CellPos, Cell)> = Vec::new();
    let mut claimed: FxHashSet<CellPos> = FxHashSet::default();
    for ly in 0..body.height {
        for lx in 0..body.width {
            let cell = body.cell_at(lx, ly);
            if cell.is_air() {
                continue;
            }
            let (wx, wy) = body.local_to_world(lx as f32 + 0.5, ly as f32 + 0.5);
            let base = CellPos::new(wx.floor() as i32, wy.floor() as i32);
            let target = [(0, 0), (0, 1), (1, 0), (-1, 0), (0, 2), (0, -1)]
                .iter()
                .map(|&(dx, dy)| base.translated(dx, dy))
                .find(|&pos| {
                    if claimed.contains(&pos)
                        || entities.iter().any(|entity| entity.contains_cell(pos))
                    {
                        return false;
                    }
                    match world.get_cell(pos) {
                        Some(existing) => !matches!(
                            registry.get(existing.material).phase,
                            Phase::Solid | Phase::Powder
                        ),
                        None => false,
                    }
                });
            let Some(pos) = target else {
                return false;
            };
            let displaced = world.get_cell(pos).expect("stamp target is loaded");
            if !displaced.is_air() {
                let Some(spot) = relocation_spot(world, registry, entities, &claimed, pos) else {
                    return false;
                };
                claimed.insert(spot);
                writes.push((spot, displaced));
            }
            claimed.insert(pos);
            writes.push((pos, cell));
        }
    }
    for (pos, cell) in writes {
        world.set_cell(pos, cell);
    }
    true
}

const SALT_BODY_REACT: u32 = 7;
const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

pub fn react_body(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    body: &mut PixelBody,
    tick: u64,
) -> bool {
    let mut mutated = false;
    for &(lx, ly) in &body.perimeter {
        let index = ly as usize * body.width as usize + lx as usize;
        let cell = body.cells[index];
        if cell.is_air() || !registry.is_reactive(cell.material) {
            continue;
        }
        let (wx, wy) = body.local_to_world(lx as f32 + 0.5, ly as f32 + 0.5);
        let pos = CellPos::new(wx.floor() as i32, wy.floor() as i32);
        for (dx, dy) in NEIGHBORS {
            let neighbor_pos = pos.translated(dx, dy);
            let Some(neighbor) = world.get_cell(neighbor_pos) else {
                continue;
            };
            let Some(reaction) = registry.reaction(cell.material, neighbor.material) else {
                continue;
            };
            if !roll(pos, tick, SALT_BODY_REACT, reaction.chance) {
                continue;
            }
            let product = reaction.becomes;
            let solid_product =
                product != MaterialId::AIR && registry.get(product).phase == Phase::Solid;
            let needs_spot = product != MaterialId::AIR && !solid_product;
            let spawn_spot = if needs_spot {
                match escape_spot(world, registry, pos) {
                    Some(spot) => Some(spot),
                    None => continue,
                }
            } else {
                None
            };
            world.set_cell(
                neighbor_pos,
                product_cell(reaction.other_becomes, neighbor_pos, tick),
            );
            if solid_product {
                body.cells[index] = product_cell(product, pos, tick);
            } else {
                body.cells[index] = Cell::AIR;
                if let Some(spot) = spawn_spot {
                    world.set_cell(spot, product_cell(product, spot, tick));
                }
            }
            mutated = true;
            break;
        }
    }
    mutated
}

fn escape_spot(world: &CellWorld, registry: &MaterialRegistry, pos: CellPos) -> Option<CellPos> {
    [(0, 0), (0, 1), (1, 0), (-1, 0), (0, -1)]
        .iter()
        .map(|&(dx, dy)| pos.translated(dx, dy))
        .find(|&spot| {
            world
                .get_cell(spot)
                .is_some_and(|cell| registry.get(cell.material).phase == Phase::Empty)
        })
}

fn product_cell(material: MaterialId, pos: CellPos, tick: u64) -> Cell {
    let mut hasher = rustc_hash::FxHasher::default();
    (pos.x, pos.y, tick).hash(&mut hasher);
    Cell::new(material, (hasher.finish() & 0xF) as u8)
}

fn roll(pos: CellPos, tick: u64, salt: u32, chance: f32) -> bool {
    if chance <= 0.0 {
        return false;
    }
    let mut hasher = rustc_hash::FxHasher::default();
    (pos.x, pos.y, tick, salt).hash(&mut hasher);
    ((hasher.finish() as u32) as f32) < chance * u32::MAX as f32
}

pub fn stamp_body(world: &mut CellWorld, registry: &MaterialRegistry, body: &PixelBody) {
    for ly in 0..body.height {
        for lx in 0..body.width {
            let cell = body.cell_at(lx, ly);
            if cell.is_air() {
                continue;
            }
            let (wx, wy) = body.local_to_world(lx as f32 + 0.5, ly as f32 + 0.5);
            let base = CellPos::new(wx.floor() as i32, wy.floor() as i32);
            let target = [(0, 0), (0, 1), (1, 0), (-1, 0), (0, 2), (0, -1)]
                .iter()
                .map(|&(dx, dy)| base.translated(dx, dy))
                .find(|&pos| match world.get_cell(pos) {
                    Some(existing) => !matches!(
                        registry.get(existing.material).phase,
                        Phase::Solid | Phase::Powder
                    ),
                    None => false,
                });
            if let Some(pos) = target {
                world.set_cell(pos, cell);
            }
        }
    }
}

fn relocation_spot(
    world: &CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityBox],
    claimed: &FxHashSet<CellPos>,
    from: CellPos,
) -> Option<CellPos> {
    for radius in 1..=STAMP_RELOCATE_RADIUS {
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
            if claimed.contains(&pos) || entities.iter().any(|entity| entity.contains_cell(pos)) {
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
