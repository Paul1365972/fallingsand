use super::{
    OwnerMap, PixelBody, Raster, angle_steps_for, cell_mass, commit_stamp, rasterize_at,
    relocation_spot,
};
use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{Cell, CellPos, Phase, Subcell};
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

pub const MAX_BODY_EXTENT: u8 = 48;
const MAX_ISLAND_CELLS: usize = 2048;

fn pivot_of(width: u8, height: u8) -> (i32, i32) {
    ((width as i32 - 1) / 2, (height as i32 - 1) / 2)
}

struct Shape {
    mass: f32,
    com: (f32, f32),
    inertia: f32,
    restitution: f32,
    perimeter: Vec<(u8, u8)>,
}

fn derive_shape(cells: &[Cell], width: u8, height: u8) -> Option<Shape> {
    let mut mass = 0.0f32;
    let mut com = (0.0f32, 0.0f32);
    let mut restitution = 0.0f32;
    for ly in 0..height {
        for lx in 0..width {
            let cell = cells[ly as usize * width as usize + lx as usize];
            if cell.is_air() {
                continue;
            }
            let m = cell_mass(cell.material);
            mass += m;
            com.0 += m * (lx as f32 + 0.5);
            com.1 += m * (ly as f32 + 0.5);
            restitution += m * content::material(cell.material).restitution;
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
            let m = cell_mass(cell.material);
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

fn is_rigid(world: &CellWorld, pos: CellPos) -> Option<Cell> {
    let cell = world.get_cell(pos)?;
    if cell.is_body() {
        return None;
    }
    (content::phase(cell.material) == Phase::Solid && content::is_rigid_capable(cell.material))
        .then_some(cell)
}

pub fn detect_island(world: &CellWorld, seed: CellPos) -> Option<Vec<CellPos>> {
    let seed_cell = is_rigid(world, seed)?;
    let mut visited: FxHashSet<CellPos> = FxHashSet::default();
    let mut queue: VecDeque<(CellPos, fallingsand_core::MaterialId)> = VecDeque::new();
    visited.insert(seed);
    queue.push_back((seed, seed_cell.material));
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (seed.x, seed.x, seed.y, seed.y);

    while let Some((pos, material)) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = pos.translated(dx, dy);
            if visited.contains(&next) {
                continue;
            }
            let Some(cell) = is_rigid(world, next) else {
                continue;
            };
            if !content::bonds(material, cell.material) {
                continue;
            }
            min_x = min_x.min(next.x);
            max_x = max_x.max(next.x);
            min_y = min_y.min(next.y);
            max_y = max_y.max(next.y);
            if max_x - min_x >= MAX_BODY_EXTENT as i32
                || max_y - min_y >= MAX_BODY_EXTENT as i32
                || visited.len() >= MAX_ISLAND_CELLS
            {
                return None;
            }
            visited.insert(next);
            queue.push_back((next, cell.material));
        }
    }
    Some(visited.into_iter().collect())
}

pub fn register_body(world: &mut CellWorld, id: u32, island: &[CellPos]) -> PixelBody {
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

    let shape = derive_shape(&cells, width, height).expect("island has cells");
    let mut body = PixelBody {
        id,
        width,
        height,
        cells,
        perimeter: shape.perimeter,
        com_local: shape.com,
        pivot: pivot_of(width, height),
        angle_steps: angle_steps_for(width, height),
        x: Subcell::from_cell(min_x).add_cells(shape.com.0),
        y: Subcell::from_cell(min_y).add_cells(shape.com.1),
        vx: Subcell::ZERO,
        vy: Subcell::ZERO,
        angle: 0.0,
        spin: 0.0,
        inv_mass: 1.0 / shape.mass,
        inv_inertia: 1.0 / shape.inertia,
        restitution: shape.restitution,
        rest_secs: 0.0,
        raster: Raster::default(),
        frozen: false,
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
    bodies: &mut Vec<PixelBody>,
    mut notes: Vec<CellPos>,
    mut next_id: impl FnMut() -> u32,
) {
    notes.sort_unstable_by_key(|pos| (pos.y, pos.x));
    notes.dedup();
    let mut owners = OwnerMap::default();
    owners.rebuild(bodies);
    let mut removed: FxHashSet<CellPos> = FxHashSet::default();
    let mut touched: FxHashSet<usize> = FxHashSet::default();
    for pos in notes {
        let Some(index) = owners.get(pos) else {
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
            && content::phase(world_cell.material) == Phase::Solid;
        if adopt {
            body.cells[local] = world_cell;
            let mut flagged = world_cell;
            flagged.set_body(true);
            world.set_cell_raw(pos, flagged);
        } else {
            body.cells[local] = Cell::AIR;
            removed.insert(pos);
        }
        touched.insert(index);
    }

    let mut touched: Vec<usize> = touched.into_iter().collect();
    touched.sort_unstable_by(|a, b| b.cmp(a));
    for &index in &touched {
        let raster = &mut bodies[index].raster;
        raster.cells.retain(|(pos, _)| !removed.contains(pos));
        raster.set.retain(|pos| !removed.contains(pos));
    }
    for index in touched {
        let body = bodies.swap_remove(index);
        let parts = split_body(world, body, &mut next_id);
        bodies.extend(parts);
    }
}

pub struct BodyParts {
    pub width: u8,
    pub height: u8,
    pub cells: Vec<Cell>,
    pub x: Subcell,
    pub y: Subcell,
    pub vx: Subcell,
    pub vy: Subcell,
    pub angle: f32,
    pub spin: f32,
    pub rest_secs: f32,
}

pub fn body_parts(body: &PixelBody) -> BodyParts {
    BodyParts {
        width: body.width,
        height: body.height,
        cells: body.cells.clone(),
        x: body.x,
        y: body.y,
        vx: body.vx,
        vy: body.vy,
        angle: body.angle,
        spin: body.spin,
        rest_secs: body.rest_secs,
    }
}

pub fn unstamp_body(world: &mut CellWorld, body: &PixelBody) {
    for &(pos, _) in &body.raster.cells {
        world.set_cell_raw(pos, Cell::AIR);
    }
}

pub fn stamp_raster(world: &mut CellWorld, body: &PixelBody) {
    for &(pos, local) in &body.raster.cells {
        let mut cell = body.cells[local as usize];
        cell.set_body(true);
        world.set_cell_raw(pos, cell);
    }
}

pub fn revive_body(world: &mut CellWorld, id: u32, parts: BodyParts) -> Option<PixelBody> {
    let shape = derive_shape(&parts.cells, parts.width, parts.height)?;
    let mut body = PixelBody {
        id,
        width: parts.width,
        height: parts.height,
        cells: parts.cells,
        perimeter: shape.perimeter,
        com_local: shape.com,
        pivot: pivot_of(parts.width, parts.height),
        angle_steps: angle_steps_for(parts.width, parts.height),
        x: parts.x,
        y: parts.y,
        vx: parts.vx,
        vy: parts.vy,
        angle: parts.angle,
        spin: parts.spin,
        inv_mass: 1.0 / shape.mass,
        inv_inertia: 1.0 / shape.inertia,
        restitution: shape.restitution,
        rest_secs: parts.rest_secs,
        raster: Raster::default(),
        frozen: false,
    };
    let raster = rasterize_at(&body, body.x, body.y, body.angle);
    let cells = &body.cells;
    let cell_for = |local: u16| {
        let mut cell = cells[local as usize];
        cell.set_body(true);
        cell
    };
    if commit_stamp(world, &[], &Raster::default(), &raster, &cell_for).is_some() {
        body.raster = raster;
        return Some(body);
    }

    let mut claimed: FxHashSet<CellPos> = FxHashSet::default();
    let exclude: FxHashSet<CellPos> = FxHashSet::default();
    for &(pos, local) in &raster.cells {
        let mut cell = cells[local as usize];
        cell.set_body(false);
        let target = match world.get_cell(pos) {
            Some(existing)
                if content::phase(existing.material) == Phase::Empty && !claimed.contains(&pos) =>
            {
                Some(pos)
            }
            _ => relocation_spot(world, &[], &claimed, &exclude, pos),
        };
        if let Some(target) = target {
            claimed.insert(target);
            world.set_cell_raw(target, cell);
        }
    }
    None
}

fn split_body(
    world: &mut CellWorld,
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
                if component[neighbor] == 0
                    && !body.cells[neighbor].is_air()
                    && content::bonds(body.cells[index].material, body.cells[neighbor].material)
                {
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
        let Some(shape) = derive_shape(&cells, part_w, part_h) else {
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
            pivot: pivot_of(part_w, part_h),
            angle_steps: angle_steps_for(part_w, part_h),
            x: body.x.add_cells(rx),
            y: body.y.add_cells(ry),
            vx: body.vx.add_cells_per_second(-body.spin * ry),
            vy: body.vy.add_cells_per_second(body.spin * rx),
            angle: body.angle,
            spin: body.spin,
            inv_mass: 1.0 / shape.mass,
            inv_inertia: 1.0 / shape.inertia,
            restitution: shape.restitution,
            rest_secs: 0.0,
            raster: Raster::default(),
            frozen: false,
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
