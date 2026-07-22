use super::rotation::{quantize_step, unrotate_offset};
use super::{BodyCell, PixelBody, Raster, angle_steps_for, cell_mass};
use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{Cell, CellPos, Phase, Subcell};
use fallingsand_math::{SUBCELL_UNITS_PER_CELL, TICK_RATE};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

const MAX_BODY_EXTENT: u8 = 48;
const MAX_ISLAND_CELLS: usize = 2048;

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
        if externally_supported(world, pos, material) {
            return None;
        }
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
    let mut island: Vec<_> = visited.into_iter().collect();
    island.sort_unstable_by_key(|pos| (pos.y, pos.x));
    Some(island)
}

fn externally_supported(
    world: &CellWorld,
    pos: CellPos,
    material: fallingsand_core::MaterialId,
) -> bool {
    let below = pos.translated(0, -1);
    if is_rigid(world, below).is_some_and(|cell| content::bonds(material, cell.material)) {
        return false;
    }
    world.get_cell(below).is_some_and(|support| {
        matches!(
            content::phase(support.material),
            Phase::Solid | Phase::Powder
        )
    })
}

pub(super) fn register_body(
    world: &mut CellWorld,
    id: u32,
    island: &[CellPos],
    angle: f32,
    offset: Option<(Subcell, Subcell)>,
) -> PixelBody {
    let mut positions = island.to_vec();
    positions.sort_unstable_by_key(|pos| (pos.y, pos.x));
    let (width, height) = dimensions(&positions);
    let grid_com = grid_com(world, &positions).expect("body island has mass");
    let pivot = choose_pivot(&positions, grid_com);
    let mut raster = Raster {
        pivot: Some(pivot),
        ..Raster::default()
    };
    for (index, &pos) in positions.iter().enumerate() {
        raster.set.insert(pos);
        raster.cells.push((pos, index as u16));
        let mut cell = world.get_cell(pos).expect("body island is loaded");
        cell.set_body(true);
        world.set_cell_raw(pos, cell);
    }
    let base_x = Subcell::from_cells(grid_com.0);
    let base_y = Subcell::from_cells(grid_com.1);
    let (offset_x, offset_y) = offset.unwrap_or((Subcell::ZERO, Subcell::ZERO));
    let mut body = PixelBody {
        id,
        width,
        height,
        pivot,
        angle_steps: angle_steps_for(width, height),
        x: base_x + offset_x,
        y: base_y + offset_y,
        vx: Subcell::ZERO,
        vy: Subcell::ZERO,
        angle,
        spin: 0.0,
        inv_mass: 0.0,
        inv_inertia: 0.0,
        restitution: 0.0,
        rest_secs: 0.0,
        liquid_resting: false,
        raster,
        cells: Vec::with_capacity(positions.len()),
        perimeter: Vec::new(),
        frozen: false,
    };
    derive_body(world, &mut body);
    body
}

pub(super) fn derive_body(world: &CellWorld, body: &mut PixelBody) -> bool {
    if body.raster.cells.is_empty() {
        return false;
    }
    if !body.raster.covers(body.pivot) {
        let positions: Vec<_> = body.raster.cells.iter().map(|&(pos, _)| pos).collect();
        let Some(com) = grid_com(world, &positions) else {
            return false;
        };
        body.pivot = choose_pivot(&positions, com);
    }

    let old_step = quantize_step(body.angle, body.angle_steps);
    let mut positions: Vec<_> = body.raster.cells.iter().map(|&(pos, _)| pos).collect();
    positions.sort_unstable_by_key(|pos| (pos.y, pos.x));
    let origin = positions[0];
    body.cells.clear();
    let mut mass = 0.0f32;
    let mut com = (0.0f32, 0.0f32);
    let mut velocity = (0.0f32, 0.0f32);
    let mut restitution = 0.0f32;
    for &pos in &positions {
        let Some(cell) = world.get_cell(pos).filter(|cell| cell.is_body()) else {
            continue;
        };
        let cell_mass = cell_mass(cell.material);
        let local = unrotate_offset(
            old_step,
            body.angle_steps,
            pos.x - body.pivot.x,
            pos.y - body.pivot.y,
        );
        body.cells.push(BodyCell {
            cell,
            local,
            mass: cell_mass,
        });
        mass += cell_mass;
        com.0 += cell_mass * (pos.x - origin.x) as f32;
        com.1 += cell_mass * (pos.y - origin.y) as f32;
        let scale = TICK_RATE as f32 / SUBCELL_UNITS_PER_CELL as f32;
        velocity.0 += cell_mass * cell.vx as f32 * scale;
        velocity.1 += cell_mass * cell.vy as f32 * scale;
        restitution += cell_mass * content::material(cell.material).restitution;
    }
    if mass <= 0.0 || body.cells.len() != positions.len() {
        return false;
    }
    com.0 /= mass;
    com.1 /= mass;
    velocity.0 /= mass;
    velocity.1 /= mass;
    restitution /= mass;

    let mut angular_momentum = 0.0f32;
    let mut inertia = 0.0f32;
    for (&pos, body_cell) in positions.iter().zip(&body.cells) {
        let rx = (pos.x - origin.x) as f32 - com.0;
        let ry = (pos.y - origin.y) as f32 - com.1;
        let scale = TICK_RATE as f32 / SUBCELL_UNITS_PER_CELL as f32;
        let vx = body_cell.cell.vx as f32 * scale;
        let vy = body_cell.cell.vy as f32 * scale;
        angular_momentum += body_cell.mass * (rx * vy - ry * vx);
        inertia += body_cell.mass * (rx * rx + ry * ry);
    }

    body.vx = Subcell::from_cells_per_second(velocity.0);
    body.vy = Subcell::from_cells_per_second(velocity.1);
    body.spin = if inertia > 0.0 {
        angular_momentum / inertia
    } else {
        0.0
    };
    body.inv_mass = 1.0 / mass;
    body.inv_inertia = if inertia > 0.0 { 1.0 / inertia } else { 0.0 };
    body.restitution = restitution;
    body.raster.cells.clear();
    for (index, &pos) in positions.iter().enumerate() {
        body.raster.cells.push((pos, index as u16));
    }
    body.perimeter.clear();
    body.perimeter
        .extend(positions.iter().copied().filter(|&pos| {
            [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .any(|(dx, dy)| !body.raster.covers(pos.translated(dx, dy)))
        }));
    (body.width, body.height) = dimensions(&positions);
    true
}

pub(super) fn apply_damage(
    world: &mut CellWorld,
    bodies: &mut Vec<PixelBody>,
    notes: &mut Vec<CellPos>,
    mut next_id: impl FnMut() -> u32,
) {
    let notes: FxHashSet<_> = notes.drain(..).collect();
    let mut touched = Vec::new();
    for (index, body) in bodies.iter_mut().enumerate() {
        let affected: Vec<_> = body
            .raster
            .cells
            .iter()
            .filter_map(|&(pos, _)| notes.contains(&pos).then_some(pos))
            .collect();
        if affected.is_empty() {
            continue;
        }
        for pos in affected {
            let world_cell = world.get_cell(pos).unwrap_or(Cell::AIR);
            let adopt = !world_cell.is_air()
                && !world_cell.is_body()
                && content::phase(world_cell.material) == Phase::Solid;
            if adopt {
                let mut flagged = world_cell;
                flagged.set_body(true);
                world.set_cell_raw(pos, flagged);
            } else {
                body.raster.cells.retain(|&(member, _)| member != pos);
                body.raster.set.remove(&pos);
            }
        }
        touched.push(index);
    }

    touched.sort_unstable_by(|a, b| b.cmp(a));
    for index in touched {
        let body = bodies.swap_remove(index);
        let offset = pose_offset(world, &body);
        let components = split_components(world, &body);
        let inherited = inherited_component(&components, body.pivot);
        for (component_index, component) in components.into_iter().enumerate() {
            let inherits = component_index == inherited;
            bodies.push(register_body(
                world,
                if inherits { body.id } else { next_id() },
                &component,
                body.angle,
                if inherits { offset } else { None },
            ));
        }
    }
}

pub(super) fn inherited_component(components: &[Vec<CellPos>], pivot: CellPos) -> usize {
    components
        .iter()
        .enumerate()
        .min_by_key(|(_, component)| {
            (
                std::cmp::Reverse(component.len()),
                !component.contains(&pivot),
                component.first().map_or(i32::MAX, |pos| pos.y),
                component.first().map_or(i32::MAX, |pos| pos.x),
            )
        })
        .map_or(usize::MAX, |(index, _)| index)
}

pub(super) fn split_components(world: &CellWorld, body: &PixelBody) -> Vec<Vec<CellPos>> {
    let step = quantize_step(body.angle, body.angle_steps);
    let mut members: FxHashMap<(i32, i32), (CellPos, fallingsand_core::MaterialId)> =
        FxHashMap::default();
    for &(pos, _) in &body.raster.cells {
        let Some(cell) = world.get_cell(pos).filter(|cell| cell.is_body()) else {
            continue;
        };
        let local = unrotate_offset(
            step,
            body.angle_steps,
            pos.x - body.pivot.x,
            pos.y - body.pivot.y,
        );
        members.insert(local, (pos, cell.material));
    }
    let mut starts: Vec<_> = members.keys().copied().collect();
    starts.sort_unstable_by_key(|&(x, y)| (y, x));
    let mut visited = FxHashSet::default();
    let mut components = Vec::new();
    for start in starts {
        if visited.contains(&start) {
            continue;
        }
        let &(start_pos, start_material) = members.get(&start).expect("body member exists");
        let mut component = Vec::new();
        let mut queue = VecDeque::new();
        visited.insert(start);
        queue.push_back((start, start_pos, start_material));
        while let Some((local, pos, material)) = queue.pop_front() {
            component.push(pos);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let next = (local.0 + dx, local.1 + dy);
                if visited.contains(&next) {
                    continue;
                }
                let Some(&(next_pos, next_material)) = members.get(&next) else {
                    continue;
                };
                if content::bonds(material, next_material) {
                    visited.insert(next);
                    queue.push_back((next, next_pos, next_material));
                }
            }
        }
        component.sort_unstable_by_key(|pos| (pos.y, pos.x));
        components.push(component);
    }
    components
}

pub(super) fn pose_offset(world: &CellWorld, body: &PixelBody) -> Option<(Subcell, Subcell)> {
    let positions: Vec<_> = body.raster.cells.iter().map(|&(pos, _)| pos).collect();
    grid_com(world, &positions).map(|com| {
        (
            body.x - Subcell::from_cells(com.0),
            body.y - Subcell::from_cells(com.1),
        )
    })
}

fn dimensions(positions: &[CellPos]) -> (u8, u8) {
    let min_x = positions.iter().map(|pos| pos.x).min().unwrap_or(0);
    let max_x = positions.iter().map(|pos| pos.x).max().unwrap_or(0);
    let min_y = positions.iter().map(|pos| pos.y).min().unwrap_or(0);
    let max_y = positions.iter().map(|pos| pos.y).max().unwrap_or(0);
    (
        (max_x - min_x + 1).clamp(1, u8::MAX as i32) as u8,
        (max_y - min_y + 1).clamp(1, u8::MAX as i32) as u8,
    )
}

fn grid_com(world: &CellWorld, positions: &[CellPos]) -> Option<(f32, f32)> {
    let mut mass = 0.0;
    let mut com = (0.0, 0.0);
    for &pos in positions {
        let cell = world.get_cell(pos)?;
        let cell_mass = cell_mass(cell.material);
        mass += cell_mass;
        com.0 += cell_mass * (pos.x as f32 + 0.5);
        com.1 += cell_mass * (pos.y as f32 + 0.5);
    }
    (mass > 0.0).then_some((com.0 / mass, com.1 / mass))
}

fn choose_pivot(positions: &[CellPos], com: (f32, f32)) -> CellPos {
    positions
        .iter()
        .copied()
        .min_by(|a, b| {
            let da = (a.x as f32 + 0.5 - com.0).powi(2) + (a.y as f32 + 0.5 - com.1).powi(2);
            let db = (b.x as f32 + 0.5 - com.0).powi(2) + (b.y as f32 + 0.5 - com.1).powi(2);
            da.total_cmp(&db).then_with(|| (a.y, a.x).cmp(&(b.y, b.x)))
        })
        .expect("body has a pivot member")
}
