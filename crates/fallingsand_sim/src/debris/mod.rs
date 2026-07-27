mod body;
mod contact;
mod island;
mod rotation;
mod rounds;

use crate::window::BodyImpulse;
use crate::world::CellWorld;
use body::{Debris, Slot, bondable, capture, release};
use fallingsand_core::{CellPos, ChunkPos, RegionPos, Subcell, content};
use fallingsand_math::round_div;
use rustc_hash::FxHashMap;
use std::collections::VecDeque;

pub struct CreaturePeer {
    pub mass_milli: i64,
    pub vx: Subcell,
    pub vy: Subcell,
    pub grounded: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct CreatureImpulse {
    pub body: u32,
    pub dvx: Subcell,
    pub dvy: Subcell,
}

#[derive(Default)]
pub struct DebrisWorld {
    bodies: Vec<Debris>,
    by_id: FxHashMap<u32, usize>,
    pending: Vec<CellPos>,
}

impl DebrisWorld {
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    pub fn rasters(&self) -> impl Iterator<Item = (u32, &[CellPos])> {
        self.bodies
            .iter()
            .map(|body| (body.id, body.raster.as_slice()))
    }

    pub fn unseat(&mut self, seeds: impl IntoIterator<Item = CellPos>) {
        self.pending.extend(seeds);
    }

    pub fn step(
        &mut self,
        world: &mut CellWorld,
        impulses: &[BodyImpulse],
        simulated: &dyn Fn(ChunkPos) -> bool,
        creature: &dyn Fn(u32) -> Option<CreaturePeer>,
        allocate: &mut dyn FnMut() -> u32,
    ) -> Vec<CreatureImpulse> {
        for body in &mut self.bodies {
            body.parked = false;
        }
        self.reconcile(world, allocate);
        self.detach(world, simulated, allocate);
        self.rebuild_index();

        for impulse in impulses {
            if let Some(&index) = self.by_id.get(&impulse.id) {
                let debris = &mut self.bodies[index];
                let com = debris.com();
                debris.apply_impulse(com, impulse.pos, impulse.jx, impulse.jy);
            }
        }
        for debris in &mut self.bodies {
            if !debris.parked {
                rounds::integrate_forces(world, debris);
            }
        }

        let mut creatures = FxHashMap::default();
        let mut cells = FxHashMap::default();
        rounds::run_rounds(
            world,
            &mut self.bodies,
            &self.by_id,
            &simulated,
            creature,
            &mut creatures,
            &mut cells,
        );

        let mut yielded: Vec<_> = cells.into_iter().collect();
        yielded.sort_unstable_by_key(|(pos, _)| (pos.y, pos.x));
        for (pos, state) in yielded {
            if (state.vx, state.vy) == (state.start_vx, state.start_vy) {
                continue;
            }
            let Some(mut cell) = world.get_cell(pos) else {
                continue;
            };
            if cell.body_id().is_some() {
                continue;
            }
            cell.set_vel(state.vx as i32, state.vy as i32);
            world.set(pos, cell);
        }

        for debris in &mut self.bodies {
            rounds::carriage(world, debris);
        }

        let mut index = 0;
        while index < self.bodies.len() {
            if rounds::try_settle(world, &mut self.bodies[index]) {
                let debris = self.bodies.remove(index);
                settle_body(world, &debris);
            } else {
                index += 1;
            }
        }
        self.rebuild_index();

        let mut shoves: Vec<CreatureImpulse> = creatures
            .into_iter()
            .filter_map(|(id, state)| {
                let dvx = state.vx - state.start_vx;
                let dvy = state.vy - state.start_vy;
                (dvx != 0 || dvy != 0).then_some(CreatureImpulse {
                    body: id,
                    dvx: Subcell::from_raw(dvx),
                    dvy: Subcell::from_raw(dvy),
                })
            })
            .collect();
        shoves.sort_unstable_by_key(|shove| shove.body);
        shoves
    }

    pub fn creature_collide(
        &mut self,
        id: u32,
        cells: &[(CellPos, i64)],
        horizontal: bool,
        removed: Subcell,
        before: Subcell,
        creature_mass: i64,
    ) -> Option<Subcell> {
        let &index = self.by_id.get(&id)?;
        let sign = removed.raw().signum();
        if sign == 0 || cells.is_empty() {
            return Some(Subcell::ZERO);
        }
        let debris = &mut self.bodies[index];
        let com = debris.com();
        let normal = if horizontal {
            (sign as i32, 0)
        } else {
            (0, sign as i32)
        };
        let total_weight: i64 = cells.iter().map(|&(_, weight)| weight.max(1)).sum();
        let mut mean = (0i128, 0i128);
        for &(pos, weight) in cells {
            mean.0 += i128::from(body::cell_center(pos.x)) * i128::from(weight.max(1));
            mean.1 += i128::from(body::cell_center(pos.y)) * i128::from(weight.max(1));
        }
        let point = (
            round_div(mean.0, i128::from(total_weight)) as i64,
            round_div(mean.1, i128::from(total_weight)) as i64,
        );
        let point_velocity = (
            debris.vx - debris.spin.speed_at(point.1 - com.1),
            debris.vy + debris.spin.speed_at(point.0 - com.0),
        );
        let peer_velocity = if horizontal {
            point_velocity.0
        } else {
            point_velocity.1
        };
        let closing = ((before.raw() - peer_velocity) * sign).clamp(0, removed.raw() * sign);
        if closing == 0 {
            return Some(removed);
        }
        let effective = contact::debris_point_mass(debris, com, point, normal);
        let mass = i128::from(creature_mass.max(1));
        let pair = (mass * effective / (mass + effective)).max(1);
        let magnitude = i128::from(closing) * pair;
        for &(pos, weight) in cells {
            let share = magnitude * i128::from(weight.max(1)) / i128::from(total_weight);
            let jx = (share * i128::from(normal.0)) as i64;
            let jy = (share * i128::from(normal.1)) as i64;
            debris.apply_impulse(com, pos, jx, jy);
        }
        let spent = round_div(magnitude, mass) as i64 * sign;
        let give_back = if sign > 0 {
            (removed.raw() - spent).clamp(0, removed.raw())
        } else {
            (removed.raw() - spent).clamp(removed.raw(), 0)
        };
        Some(Subcell::from_raw(give_back))
    }

    pub fn settle_regions(&mut self, world: &mut CellWorld, regions: &[RegionPos]) {
        let mut index = 0;
        while index < self.bodies.len() {
            let crossing = self.bodies[index]
                .raster
                .iter()
                .any(|pos| regions.iter().any(|&region| pos.chunk().region() == region));
            if crossing {
                let debris = self.bodies.remove(index);
                settle_body(world, &debris);
            } else {
                index += 1;
            }
        }
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        self.by_id.clear();
        for (index, body) in self.bodies.iter().enumerate() {
            self.by_id.insert(body.id, index);
        }
    }

    fn reconcile(&mut self, world: &mut CellWorld, allocate: &mut dyn FnMut() -> u32) {
        let mut index = 0;
        while index < self.bodies.len() {
            match reconcile_body(world, &self.bodies[index], allocate) {
                Reconciled::Intact => index += 1,
                Reconciled::Parked => {
                    self.bodies[index].parked = true;
                    index += 1;
                }
                Reconciled::Gone => {
                    self.bodies.remove(index);
                }
                Reconciled::Parts(mut parts) => {
                    let retained = self.bodies[index].id;
                    self.bodies.remove(index);
                    let mut offset = 0;
                    for part in parts.drain(..) {
                        if part.id == retained {
                            self.bodies.insert(index, part);
                            offset = 1;
                        } else {
                            for &pos in &part.raster {
                                if let Some(mut cell) = world.get_cell(pos)
                                    && cell.body_id() == Some(retained)
                                {
                                    cell.set_body(part.id);
                                    world.set(pos, cell);
                                }
                            }
                            self.bodies.push(part);
                        }
                    }
                    index += offset;
                }
            }
        }
    }

    fn detach(
        &mut self,
        world: &mut CellWorld,
        simulated: &dyn Fn(ChunkPos) -> bool,
        allocate: &mut dyn FnMut() -> u32,
    ) {
        let mut seeds = std::mem::take(&mut self.pending);
        seeds.sort_unstable_by_key(|pos| (pos.y, pos.x));
        seeds.dedup();
        for seed in seeds {
            let mut candidates = vec![seed];
            candidates.extend(
                fallingsand_core::CARDINAL_NEIGHBORS
                    .iter()
                    .map(|&(dx, dy)| seed.translated(dx, dy)),
            );
            for candidate in candidates {
                if world.get_cell(candidate).is_none() {
                    self.pending.push(candidate);
                    continue;
                }
                let Some(island) = island::detect_detached_island(world, candidate) else {
                    continue;
                };
                if island_simulated(world, simulated, &island) {
                    let id = allocate();
                    self.bodies.push(capture(world, id, island));
                } else {
                    self.pending.push(candidate);
                }
            }
        }
    }
}

enum Reconciled {
    Intact,
    Parked,
    Gone,
    Parts(Vec<Debris>),
}

fn reconcile_body(
    world: &mut CellWorld,
    debris: &Debris,
    allocate: &mut dyn FnMut() -> u32,
) -> Reconciled {
    let com = debris.com();
    let mut changed = false;
    let mut survivors: Vec<Slot> = Vec::with_capacity(debris.slots.len());
    let mut positions: Vec<CellPos> = Vec::with_capacity(debris.raster.len());
    for (slot, &pos) in debris.slots.iter().zip(&debris.raster) {
        let Some(cell) = world.get_cell(pos) else {
            return Reconciled::Parked;
        };
        if cell.body_id() != Some(debris.id) {
            changed = true;
            continue;
        }
        if !bondable(cell.material) {
            release(world, debris, com, pos);
            changed = true;
            continue;
        }
        if cell.material != slot.material {
            changed = true;
        }
        survivors.push(Slot {
            local: slot.local,
            material: cell.material,
        });
        positions.push(pos);
    }
    if survivors.is_empty() {
        return Reconciled::Gone;
    }
    if !changed {
        return Reconciled::Intact;
    }

    let by_local: FxHashMap<(i32, i32), u32> = survivors
        .iter()
        .enumerate()
        .map(|(slot, entry)| (entry.local, slot as u32))
        .collect();
    const UNCLAIMED: u32 = u32::MAX;
    let mut part_of = vec![UNCLAIMED; survivors.len()];
    let mut parts = 0u32;
    let mut queue = VecDeque::new();
    for start in 0..survivors.len() {
        if part_of[start] != UNCLAIMED {
            continue;
        }
        part_of[start] = parts;
        queue.push_back(start as u32);
        while let Some(current) = queue.pop_front() {
            let entry = survivors[current as usize];
            for (dx, dy) in fallingsand_core::CARDINAL_NEIGHBORS {
                let Some(&next) = by_local.get(&(entry.local.0 + dx, entry.local.1 + dy)) else {
                    continue;
                };
                if part_of[next as usize] == UNCLAIMED
                    && content::bonds(entry.material, survivors[next as usize].material)
                {
                    part_of[next as usize] = parts;
                    queue.push_back(next);
                }
            }
        }
        parts += 1;
    }

    let mut out = Vec::with_capacity(parts as usize);
    for part in 0..parts {
        let mut slots = Vec::new();
        let mut raster = Vec::new();
        for (slot, &assigned) in part_of.iter().enumerate() {
            if assigned == part {
                slots.push(survivors[slot]);
                raster.push(positions[slot]);
            }
        }
        let id = if part == 0 { debris.id } else { allocate() };
        out.push(derive_part(debris, com, id, slots, raster));
    }
    Reconciled::Parts(out)
}

fn derive_part(
    source: &Debris,
    com: (i64, i64),
    id: u32,
    slots: Vec<Slot>,
    raster: Vec<CellPos>,
) -> Debris {
    let mut debris = Debris {
        id,
        slots,
        raster,
        anchor: source.anchor,
        step: source.step,
        vx: source.vx,
        vy: source.vy,
        spin: source.spin,
        acc_x: if id == source.id { source.acc_x } else { 0 },
        acc_y: if id == source.id { source.acc_y } else { 0 },
        acc_turn: if id == source.id { source.acc_turn } else { 0 },
        mass: 0,
        moment: 0,
        radius: 0,
        restitution: fallingsand_core::Q16::from_raw(0),
        friction: fallingsand_core::Q16::from_raw(0),
        parked: false,
    };
    debris.refresh_inertia();
    let part_com = debris.com();
    debris.vx = source.vx - source.spin.speed_at(part_com.1 - com.1);
    debris.vy = source.vy + source.spin.speed_at(part_com.0 - com.0);
    debris
}

fn settle_body(world: &mut CellWorld, debris: &Debris) {
    for &pos in &debris.raster {
        let Some(mut cell) = world.get_cell(pos) else {
            continue;
        };
        if cell.body_id() != Some(debris.id) {
            continue;
        }
        cell.clear_body();
        cell.set_vel(0, 0);
        world.set(pos, cell);
    }
}

fn island_simulated(
    world: &CellWorld,
    simulated: &dyn Fn(ChunkPos) -> bool,
    island: &[CellPos],
) -> bool {
    let min_x = island.iter().map(|pos| pos.x).min().unwrap();
    let max_x = island.iter().map(|pos| pos.x).max().unwrap();
    let min_y = island.iter().map(|pos| pos.y).min().unwrap();
    let max_y = island.iter().map(|pos| pos.y).max().unwrap();
    let min = CellPos::new(min_x - 1, min_y - 1).chunk();
    let max = CellPos::new(max_x + 1, max_y + 1).chunk();
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
