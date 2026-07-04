use crate::obstacles::EntityBox;
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, MaterialRegistry, Phase};
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

pub const MAX_ISLAND_EXTENT: i32 = 48;
pub const MAX_ISLAND_CELLS: usize = 2048;
pub const SETTLE_TICKS: u8 = 20;
const SETTLE_SPEED_SQ: f32 = 100.0;
const SETTLE_SPIN: f32 = 1.5;
const CONTACT_DAMPING: f32 = 0.94;
const RESTITUTION: f32 = 0.0;
const FRICTION: f32 = 0.4;
const CONTACT_ITERATIONS: usize = 4;
const MAX_CONTACTS: usize = 8;
const PENETRATION_CORRECTION: f32 = 0.5;

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
    pub rest_ticks: u8,
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
    let mut mass = 0.0f32;
    let mut com = (0.0f32, 0.0f32);
    for pos in island {
        let cell = world.get_cell(*pos).unwrap();
        let (lx, ly) = ((pos.x - min_x) as usize, (pos.y - min_y) as usize);
        cells[ly * width as usize + lx] = cell;
        mass += 1.0;
        com.0 += lx as f32 + 0.5;
        com.1 += ly as f32 + 0.5;
        world.place_material(*pos, fallingsand_core::MaterialId::AIR);
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
            let (dx, dy) = (lx as f32 + 0.5 - com.0, ly as f32 + 0.5 - com.1);
            inertia += dx * dx + dy * dy + 1.0 / 6.0;
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

    let world_com = (min_x as f32 + com.0, min_y as f32 + com.1);
    let _ = registry;
    PixelBody {
        id,
        width,
        height,
        cells,
        perimeter,
        com_local: com,
        x: world_com.0,
        y: world_com.1,
        vx: 0.0,
        vy: 0.0,
        angle: 0.0,
        spin: 0.0,
        inv_mass: 1.0 / mass,
        inv_inertia: 1.0 / inertia,
        rest_ticks: 0,
    }
}

struct Contact {
    rx: f32,
    ry: f32,
    nx: f32,
    ny: f32,
    depth: f32,
}

fn obstructed(
    world: &CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityBox],
    pos: CellPos,
) -> bool {
    blocks_body(world, registry, pos) || entities.iter().any(|entity| entity.contains_cell(pos))
}

fn find_contacts(
    world: &CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityBox],
    body: &PixelBody,
) -> Vec<Contact> {
    let mut contacts: Vec<Contact> = Vec::new();
    for &(lx, ly) in &body.perimeter {
        let (wx, wy) = body.local_to_world(lx as f32 + 0.5, ly as f32 + 0.5);
        let pos = CellPos::new(wx.floor() as i32, wy.floor() as i32);
        if !obstructed(world, registry, entities, pos) {
            continue;
        }
        let mut nx = 0.0f32;
        let mut ny = 0.0f32;
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                if (dx, dy) == (0, 0) {
                    continue;
                }
                if !obstructed(world, registry, entities, pos.translated(dx, dy)) {
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
            depth: 0.5,
        });
        if contacts.len() >= MAX_CONTACTS {
            break;
        }
    }
    contacts
}

pub fn step_body(
    world: &CellWorld,
    registry: &MaterialRegistry,
    entities: &[EntityBox],
    body: &mut PixelBody,
    gravity: f32,
    dt: f32,
) -> bool {
    let (prev_x, prev_y, prev_angle) = (body.x, body.y, body.angle);
    body.vy += gravity * dt;
    body.x += body.vx * dt;
    body.y += body.vy * dt;
    body.angle += body.spin * dt;

    let contacts = find_contacts(world, registry, entities, body);
    let touching = !contacts.is_empty();

    for _ in 0..CONTACT_ITERATIONS {
        for contact in &contacts {
            let rel_vx = body.vx - body.spin * contact.ry;
            let rel_vy = body.vy + body.spin * contact.rx;
            let vn = rel_vx * contact.nx + rel_vy * contact.ny;
            if vn >= 0.0 {
                continue;
            }
            let r_cross_n = contact.rx * contact.ny - contact.ry * contact.nx;
            let k = body.inv_mass + r_cross_n * r_cross_n * body.inv_inertia;
            let jn = -(1.0 + RESTITUTION) * vn / k;
            body.vx += jn * contact.nx * body.inv_mass;
            body.vy += jn * contact.ny * body.inv_mass;
            body.spin += r_cross_n * jn * body.inv_inertia;

            let tx = -contact.ny;
            let ty = contact.nx;
            let rel_vx = body.vx - body.spin * contact.ry;
            let rel_vy = body.vy + body.spin * contact.rx;
            let vt = rel_vx * tx + rel_vy * ty;
            let r_cross_t = contact.rx * ty - contact.ry * tx;
            let kt = body.inv_mass + r_cross_t * r_cross_t * body.inv_inertia;
            let jt = (-vt / kt).clamp(-FRICTION * jn.abs(), FRICTION * jn.abs());
            body.vx += jt * tx * body.inv_mass;
            body.vy += jt * ty * body.inv_mass;
            body.spin += r_cross_t * jt * body.inv_inertia;
        }
    }

    if touching {
        body.vx *= CONTACT_DAMPING;
        body.vy *= CONTACT_DAMPING;
        body.spin *= CONTACT_DAMPING;
    }

    let slow =
        body.vx * body.vx + body.vy * body.vy < SETTLE_SPEED_SQ && body.spin.abs() < SETTLE_SPIN;
    if touching && slow {
        body.x = prev_x;
        body.y = prev_y;
        body.angle = prev_angle;
        body.vx = 0.0;
        body.vy = 0.0;
        body.spin = 0.0;
    } else if let Some(deepest) = contacts.first() {
        let correction = deepest.depth * PENETRATION_CORRECTION;
        body.x += deepest.nx * correction;
        body.y += deepest.ny * correction;
    }
    let radius = 0.5 * (body.width as f32).hypot(body.height as f32);
    let near_entity = entities.iter().any(|entity| {
        (body.x - entity.x).abs() < radius + entity.half_w + 1.0
            && (body.y - entity.y).abs() < radius + entity.half_h + 1.0
    });
    if touching && slow && !near_entity {
        body.rest_ticks = body.rest_ticks.saturating_add(1);
    } else {
        body.rest_ticks = 0;
    }
    body.rest_ticks >= SETTLE_TICKS
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
