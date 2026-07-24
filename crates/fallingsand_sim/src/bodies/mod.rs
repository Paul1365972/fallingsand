mod island;
mod rotation;

pub use island::detect_detached_island;

use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, Phase, RegionPos, Subcell, content};
use fallingsand_math::SUBCELL_UNITS_PER_CELL;
use rotation::{TURN_UNITS, quantize_step, rotate_offset};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

const MAX_TRAVEL_CELLS: i64 = 64;
const MAX_LINEAR_STEP: Subcell = Subcell::from_cell(MAX_TRAVEL_CELLS as i32);
const RESPONSE_SCALE: i128 = 1 << 16;
const MIN_RESTITUTION: u32 = RESPONSE_SCALE as u32 / 20;
const FRICTION_NUMERATOR: i64 = 3;
const FRICTION_DENOMINATOR: i64 = 4;
const TAU_NUMERATOR: i128 = 710;
const TAU_DENOMINATOR: i128 = 113;
const ANGULAR_DENOMINATOR: i128 = TURN_UNITS as i128 * TAU_DENOMINATOR;
const SETTLE_LINEAR: i64 = Subcell::from_cells_per_second(8.0).raw();
const SETTLE_SPIN: i64 = TURN_UNITS / 256;

#[derive(Default)]
pub struct BodySet {
    bodies: Vec<Body>,
    scratch: Scratch,
}

#[derive(Default)]
struct Scratch {
    cells: Vec<Cell>,
    current: Vec<CellPos>,
    candidate: Vec<CellPos>,
    candidate_set: FxHashSet<CellPos>,
    by_local: FxHashMap<(i32, i32), usize>,
    visited: Vec<bool>,
    queue: VecDeque<usize>,
    components: Vec<Vec<usize>>,
}

struct Body {
    local: Vec<(i32, i32)>,
    bonds: Vec<u8>,
    raster: Vec<CellPos>,
    occupied: FxHashSet<CellPos>,
    x: Subcell,
    y: Subcell,
    vx: Subcell,
    vy: Subcell,
    angle: i64,
    spin: i64,
    radius: i64,
    inertia: i128,
    restitution: u32,
}

impl BodySet {
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    pub fn detach(&mut self, world: &mut CellWorld, island: Vec<CellPos>) {
        self.bodies.push(Body::new(world, island));
    }

    pub fn push_at(&mut self, pos: CellPos, dvx: Subcell, dvy: Subcell, source_mass: u32) -> bool {
        let Some(body) = self
            .bodies
            .iter_mut()
            .find(|body| body.occupied.contains(&pos))
        else {
            return false;
        };
        let mass = body.local.len() as i64;
        let transfer = mass.min(i64::from(source_mass));
        let jx = i128::from(dvx.raw()) * i128::from(transfer);
        let jy = i128::from(dvy.raw()) * i128::from(transfer);
        body.vx += Subcell::from_raw(round_div(jx, i128::from(mass)) as i64);
        body.vy += Subcell::from_raw(round_div(jy, i128::from(mass)) as i64);
        add_torque(body, pos, jx, jy);
        true
    }

    pub fn rasters(&self) -> impl Iterator<Item = impl Iterator<Item = CellPos> + '_> + '_ {
        self.bodies.iter().map(|body| body.raster.iter().copied())
    }

    pub fn reconcile(&mut self, world: &mut CellWorld) {
        let bodies = std::mem::take(&mut self.bodies);
        for body in bodies {
            reconcile(world, body, &mut self.scratch, &mut self.bodies);
        }
    }

    pub fn step(
        &mut self,
        world: &mut CellWorld,
        gravity: Subcell,
        simulated: impl Fn(fallingsand_core::ChunkPos) -> bool,
    ) {
        self.bodies
            .sort_unstable_by_key(|body| body.raster.iter().map(|p| p.y).min());
        let mut index = 0;
        while index < self.bodies.len() {
            capture(world, &mut self.bodies[index], &mut self.scratch.cells);
            if advance(
                world,
                &mut self.bodies[index],
                gravity,
                &simulated,
                &mut self.scratch,
            ) {
                let body = self.bodies.remove(index);
                settle(world, &body);
            } else {
                index += 1;
            }
        }
    }

    pub fn settle_regions(&mut self, world: &mut CellWorld, regions: &[RegionPos]) {
        let mut index = 0;
        while index < self.bodies.len() {
            let overlaps = self.bodies[index].raster.iter().any(|cell| {
                regions
                    .iter()
                    .any(|&region| cell.chunk().region() == region)
            });
            if overlaps {
                let body = self.bodies.remove(index);
                settle(world, &body);
            } else {
                index += 1;
            }
        }
    }
}

impl Body {
    fn new(world: &mut CellWorld, raster: Vec<CellPos>) -> Self {
        let (cx, cy) = center(raster.iter().copied());
        let pivot = raster
            .iter()
            .copied()
            .min_by_key(|pos| {
                let dx = i128::from(Subcell::cell_center(pos.x).raw() - cx);
                let dy = i128::from(Subcell::cell_center(pos.y).raw() - cy);
                (dx * dx + dy * dy, pos.y, pos.x)
            })
            .expect("body island is not empty");
        let local = raster
            .iter()
            .map(|pos| (pos.x - pivot.x, pos.y - pivot.y))
            .collect();
        let bonds = raster
            .iter()
            .map(|&pos| {
                let cell = world.get_cell(pos).expect("body island is loaded");
                content::bond_group(cell.material)
            })
            .collect();
        let occupied = raster.iter().copied().collect();
        let (radius, inertia) = geometry(&raster);
        let mut restitution = 0;
        let mut velocity = (0i128, 0i128);
        let mut angular_momentum = 0i128;
        let mut offset = (0i128, 0i128);
        let count = raster.len() as i128;
        for &pos in &raster {
            let cell = world.get_cell(pos).expect("body island is loaded");
            velocity.0 += i128::from(cell.vx);
            velocity.1 += i128::from(cell.vy);
        }
        velocity.0 = round_div(velocity.0, count);
        velocity.1 = round_div(velocity.1, count);
        for &pos in &raster {
            let mut cell = world.get_cell(pos).expect("body island is loaded");
            restitution = restitution.max(content::restitution_q16(cell.material));
            let rx = i128::from(Subcell::cell_center(pos.x).raw() - cx);
            let ry = i128::from(Subcell::cell_center(pos.y).raw() - cy);
            angular_momentum += rx * i128::from(cell.vy) - ry * i128::from(cell.vx);
            offset.0 += rx;
            offset.1 += ry;
            cell.set_body(true);
            world.set(pos, cell, false);
        }
        angular_momentum -= offset.0 * velocity.1 - offset.1 * velocity.0;
        let spin = if inertia == 0 {
            0
        } else {
            round_div(
                angular_momentum * i128::from(TURN_UNITS) * TAU_DENOMINATOR,
                inertia * TAU_NUMERATOR,
            ) as i64
        };
        Self {
            local,
            bonds,
            raster,
            occupied,
            x: Subcell::from_raw(cx),
            y: Subcell::from_raw(cy),
            vx: Subcell::from_raw(velocity.0 as i64),
            vy: Subcell::from_raw(velocity.1 as i64),
            angle: 0,
            spin,
            radius,
            inertia,
            restitution: restitution.max(MIN_RESTITUTION),
        }
    }
}

fn reconcile(world: &mut CellWorld, body: Body, scratch: &mut Scratch, out: &mut Vec<Body>) {
    let intact = body.raster.iter().enumerate().all(|(index, &pos)| {
        world.get_cell(pos).is_some_and(|cell| {
            cell.is_body()
                && content::phase(cell.material) == Phase::Solid
                && content::bond_group(cell.material) == body.bonds[index]
        })
    });
    if intact {
        out.push(body);
        return;
    }

    scratch.cells.clear();
    scratch.cells.resize(body.local.len(), Cell::AIR);
    scratch.by_local.clear();
    scratch.visited.clear();
    scratch.visited.resize(body.local.len(), true);
    for (index, &pos) in body.raster.iter().enumerate() {
        let Some(mut cell) = world.get_cell(pos) else {
            continue;
        };
        if !cell.is_body() {
            continue;
        }
        release(world, &body, pos, &mut cell);
        if content::phase(cell.material) != Phase::Solid {
            continue;
        }
        scratch.cells[index] = cell;
        scratch.by_local.insert(body.local[index], index);
        scratch.visited[index] = false;
    }
    if scratch.by_local.is_empty() {
        return;
    }

    let mut component_count = 0;
    for start in 0..body.local.len() {
        if scratch.visited[start] {
            continue;
        }
        if component_count == scratch.components.len() {
            scratch.components.push(Vec::new());
        }
        let component = &mut scratch.components[component_count];
        component.clear();
        component_count += 1;
        scratch.visited[start] = true;
        scratch.queue.push_back(start);
        while let Some(index) = scratch.queue.pop_front() {
            component.push(index);
            let material = scratch.cells[index].material;
            for (dx, dy) in fallingsand_core::CARDINAL_NEIGHBORS {
                let local = (body.local[index].0 + dx, body.local[index].1 + dy);
                let Some(&next) = scratch.by_local.get(&local) else {
                    continue;
                };
                if !scratch.visited[next] && content::bonds(material, scratch.cells[next].material)
                {
                    scratch.visited[next] = true;
                    scratch.queue.push_back(next);
                }
            }
        }
    }

    for component in &scratch.components[..component_count] {
        let raster = component.iter().map(|&index| body.raster[index]).collect();
        out.push(Body::new(world, raster));
    }
}

fn release(world: &mut CellWorld, body: &Body, pos: CellPos, cell: &mut Cell) {
    let rx = i128::from(Subcell::cell_center(pos.x).raw() - body.x.raw());
    let ry = i128::from(Subcell::cell_center(pos.y).raw() - body.y.raw());
    let vx = body.vx.raw()
        + round_div(
            -TAU_NUMERATOR * i128::from(body.spin) * ry,
            ANGULAR_DENOMINATOR,
        ) as i64;
    let vy = body.vy.raw()
        + round_div(
            TAU_NUMERATOR * i128::from(body.spin) * rx,
            ANGULAR_DENOMINATOR,
        ) as i64;
    cell.set_vel(vx as i32, vy as i32);
    cell.set_body(false);
    world.set(pos, *cell, false);
}

fn capture(world: &CellWorld, body: &mut Body, cells: &mut Vec<Cell>) {
    cells.clear();
    cells.extend(body.raster.iter().map(|&pos| {
        world
            .get_cell(pos)
            .filter(|cell| cell.is_body())
            .expect("body raster member is owned")
    }));
    body.restitution = restitution(cells);
}

fn advance(
    world: &mut CellWorld,
    body: &mut Body,
    gravity: Subcell,
    simulated: &impl Fn(fallingsand_core::ChunkPos) -> bool,
    scratch: &mut Scratch,
) -> bool {
    body.vx = clamp(body.vx);
    body.vy = clamp(body.vy + gravity);
    let max_spin = round_div(
        i128::from(MAX_TRAVEL_CELLS * TURN_UNITS) * TAU_DENOMINATOR,
        i128::from(body.radius) * TAU_NUMERATOR,
    )
    .min(i128::from(TURN_UNITS)) as i64;
    body.spin = body.spin.clamp(-max_spin, max_spin);
    let steps = traversal_steps(body);
    let mut pose = (body.x, body.y, body.angle);
    scratch.current.clone_from(&body.raster);
    let mut x_remainder = 0;
    let mut y_remainder = 0;
    let mut angle_remainder = 0;
    let mut settled = false;

    for _ in 0..steps {
        let next = (
            pose.0 + Subcell::from_raw(split_step(&mut x_remainder, body.vx.raw(), steps)),
            pose.1 + Subcell::from_raw(split_step(&mut y_remainder, body.vy.raw(), steps)),
            pose.2 + split_step(&mut angle_remainder, body.spin, steps),
        );
        rasterize(
            &body.local,
            next,
            &mut scratch.candidate,
            &mut scratch.candidate_set,
        );
        if scratch.candidate == scratch.current {
            pose = next;
            continue;
        }
        match collision(world, body, &scratch.current, &scratch.candidate, simulated) {
            Collision::Free => {
                pose = next;
                std::mem::swap(&mut scratch.current, &mut scratch.candidate);
            }
            Collision::Frontier => {
                body.vx = Subcell::ZERO;
                body.vy = Subcell::ZERO;
                body.spin = 0;
                break;
            }
            Collision::Blocked(hit) => {
                settled = respond(body, hit);
                break;
            }
        }
    }

    body.x = pose.0;
    body.y = pose.1;
    body.angle = pose.2.rem_euclid(TURN_UNITS);
    if scratch.current != body.raster {
        commit(
            world,
            body,
            &scratch.cells,
            &scratch.current,
            &mut scratch.candidate_set,
        );
        body.raster.clone_from(&scratch.current);
        body.occupied.clear();
        body.occupied.extend(body.raster.iter().copied());
    }
    settled
}

#[derive(Clone, Copy)]
struct Hit {
    pos: CellPos,
    x: bool,
    y: bool,
    restitution: u32,
}

enum Collision {
    Free,
    Frontier,
    Blocked(Hit),
}

fn collision(
    world: &CellWorld,
    body: &Body,
    current: &[CellPos],
    candidate: &[CellPos],
    simulated: &impl Fn(fallingsand_core::ChunkPos) -> bool,
) -> Collision {
    let mut hit = None;
    for (index, &pos) in candidate.iter().enumerate() {
        if pos == current[index] || body.occupied.contains(&pos) {
            continue;
        }
        if !simulated(pos.chunk()) {
            return Collision::Frontier;
        }
        let Some(cell) = world.get_cell(pos) else {
            return Collision::Frontier;
        };
        if !cell.is_body() && !matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
        {
            continue;
        }
        let dx = pos.x - current[index].x;
        let dy = pos.y - current[index].y;
        let entry = hit.get_or_insert(Hit {
            pos,
            x: false,
            y: false,
            restitution: 0,
        });
        if dx.abs() >= dy.abs() && dx != 0 {
            entry.x = true;
        } else {
            entry.y = true;
        }
        entry.restitution = entry
            .restitution
            .max(content::restitution_q16(cell.material));
    }
    hit.map_or(Collision::Free, Collision::Blocked)
}

fn respond(body: &mut Body, hit: Hit) -> bool {
    let old_vx = body.vx.raw();
    let old_vy = body.vy.raw();
    let restitution = body.restitution.max(hit.restitution);
    let mut vx = old_vx;
    let mut vy = old_vy;
    if hit.x {
        vx = reflect(vx, restitution);
        vy = vy * FRICTION_NUMERATOR / FRICTION_DENOMINATOR;
    }
    if hit.y {
        vy = reflect(vy, restitution);
        vx = vx * FRICTION_NUMERATOR / FRICTION_DENOMINATOR;
    }
    let mass = body.local.len() as i128;
    add_torque(
        body,
        hit.pos,
        mass * i128::from(vx - old_vx),
        mass * i128::from(vy - old_vy),
    );
    body.spin = -body.spin * i64::from(restitution) / RESPONSE_SCALE as i64;
    if hit.y && old_vy < 0 {
        if vy.abs() <= SETTLE_LINEAR {
            vy = 0;
        }
        if vx.abs() <= SETTLE_LINEAR {
            vx = 0;
        }
        if body.spin.abs() <= SETTLE_SPIN {
            body.spin = 0;
        }
    }
    body.vx = Subcell::from_raw(vx);
    body.vy = Subcell::from_raw(vy);
    vx == 0 && vy == 0 && body.spin == 0
}

fn add_torque(body: &mut Body, pos: CellPos, jx: i128, jy: i128) {
    if body.inertia == 0 {
        return;
    }
    let rx = i128::from(Subcell::cell_center(pos.x).raw() - body.x.raw());
    let ry = i128::from(Subcell::cell_center(pos.y).raw() - body.y.raw());
    body.spin += round_div(
        (rx * jy - ry * jx) * i128::from(TURN_UNITS) * TAU_DENOMINATOR,
        body.inertia * TAU_NUMERATOR,
    ) as i64;
}

fn traversal_steps(body: &Body) -> u32 {
    let linear = body.vx.raw().abs().max(body.vy.raw().abs());
    let angular = round_div(
        i128::from(body.spin.abs())
            * TAU_NUMERATOR
            * i128::from(body.radius)
            * i128::from(SUBCELL_UNITS_PER_CELL),
        ANGULAR_DENOMINATOR,
    ) as i64;
    ceil_div(
        i128::from(linear.max(angular)),
        i128::from(SUBCELL_UNITS_PER_CELL),
    )
    .max(1) as u32
}

fn rasterize(
    local: &[(i32, i32)],
    pose: (Subcell, Subcell, i64),
    out: &mut Vec<CellPos>,
    set: &mut FxHashSet<CellPos>,
) {
    out.clear();
    set.clear();
    let step = quantize_step(pose.2);
    let (sum_x, sum_y) = local.iter().fold((0i64, 0i64), |sum, &(x, y)| {
        let (x, y) = rotate_offset(step, x, y);
        (sum.0 + i64::from(x), sum.1 + i64::from(y))
    });
    let count = local.len() as i128;
    let mean_x = round_div(
        i128::from(sum_x) * i128::from(SUBCELL_UNITS_PER_CELL),
        count,
    ) as i64;
    let mean_y = round_div(
        i128::from(sum_y) * i128::from(SUBCELL_UNITS_PER_CELL),
        count,
    ) as i64;
    let pivot = CellPos::new(
        Subcell::from_raw(pose.0.raw() - mean_x).floor_cell(),
        Subcell::from_raw(pose.1.raw() - mean_y).floor_cell(),
    );
    for &(x, y) in local {
        let (x, y) = rotate_offset(step, x, y);
        let pos = pivot.translated(x, y);
        debug_assert!(set.insert(pos), "body rotation must be bijective");
        out.push(pos);
    }
}

fn commit(
    world: &mut CellWorld,
    body: &Body,
    cells: &[Cell],
    raster: &[CellPos],
    set: &mut FxHashSet<CellPos>,
) {
    set.clear();
    set.extend(raster.iter().copied());
    let mut displaced: Vec<_> = raster
        .iter()
        .filter(|pos| !body.occupied.contains(pos))
        .map(|&pos| world.get_cell(pos).expect("body proposal is loaded"))
        .collect();
    displaced.sort_unstable_by_key(|cell| std::cmp::Reverse(content::density_milli(cell.material)));
    let mut vacated: Vec<_> = body
        .raster
        .iter()
        .filter(|pos| !set.contains(pos))
        .copied()
        .collect();
    vacated.sort_unstable_by_key(|pos| (pos.y, pos.x));
    debug_assert_eq!(vacated.len(), displaced.len());
    for (pos, cell) in vacated.into_iter().zip(displaced) {
        world.set(pos, cell, true);
    }
    for (index, &pos) in raster.iter().enumerate() {
        if world.get_cell(pos) != Some(cells[index]) {
            world.set(pos, cells[index], true);
        }
    }
}

fn settle(world: &mut CellWorld, body: &Body) {
    for &pos in &body.raster {
        let Some(mut cell) = world.get_cell(pos).filter(|cell| cell.is_body()) else {
            continue;
        };
        cell.set_body(false);
        world.set(pos, cell, false);
    }
}

fn geometry(raster: &[CellPos]) -> (i64, i128) {
    let (cx, cy) = center(raster.iter().copied());
    let mut radius = 1;
    let mut inertia = 0;
    for &pos in raster {
        let dx = Subcell::cell_center(pos.x).raw() - cx;
        let dy = Subcell::cell_center(pos.y).raw() - cy;
        radius = radius.max((dx.abs() + dy.abs()) / i64::from(SUBCELL_UNITS_PER_CELL) + 1);
        inertia += i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy);
    }
    (radius, inertia)
}

fn center(positions: impl Iterator<Item = CellPos>) -> (i64, i64) {
    let (x, y, count) = positions.fold((0i128, 0i128, 0i128), |sum, pos| {
        (
            sum.0 + i128::from(Subcell::cell_center(pos.x).raw()),
            sum.1 + i128::from(Subcell::cell_center(pos.y).raw()),
            sum.2 + 1,
        )
    });
    (round_div(x, count) as i64, round_div(y, count) as i64)
}

fn restitution(cells: &[Cell]) -> u32 {
    cells
        .iter()
        .map(|cell| content::restitution_q16(cell.material))
        .max()
        .unwrap_or(0)
        .max(MIN_RESTITUTION)
}

fn clamp(value: Subcell) -> Subcell {
    Subcell::from_raw(
        value
            .raw()
            .clamp(-MAX_LINEAR_STEP.raw(), MAX_LINEAR_STEP.raw()),
    )
}

fn reflect(value: i64, restitution: u32) -> i64 {
    -round_div(i128::from(value) * i128::from(restitution), RESPONSE_SCALE) as i64
}

fn split_step(remainder: &mut i128, motion: i64, steps: u32) -> i64 {
    *remainder += i128::from(motion);
    let step = *remainder / i128::from(steps);
    *remainder %= i128::from(steps);
    step as i64
}

fn round_div(numerator: i128, denominator: i128) -> i128 {
    let half = denominator / 2;
    if numerator >= 0 {
        (numerator + half) / denominator
    } else {
        (numerator - half) / denominator
    }
}

fn ceil_div(numerator: i128, denominator: i128) -> i128 {
    (numerator + denominator - 1) / denominator
}
