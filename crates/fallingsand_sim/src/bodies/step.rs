use super::contact::{BodyBins, Other, find_contacts};
use super::solver::{
    Partner, PointState, SolverContact, SolverScratch, relative_vn, slot_for, solve_contact,
    state_of,
};
use super::{
    ActorDynamics, PixelBody, REFERENCE_DENSITY_MILLI, Raster, append_vacated_wake_targets,
    commit_stamp, rasterize_into, relocation_spot,
};
use crate::physics::{ActorAabb, BOUNCE_MIN_SPEED, fluid_drag};
use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{CARDINAL_NEIGHBORS, CellPos, ChunkPos, Phase, Subcell, TICK_DT};
use std::time::Instant;

pub(super) const SETTLE_SECS: f32 = 0.5;
const SETTLE_SPEED_SQ: f32 = 100.0;
const SETTLE_SPIN: f32 = 1.5;
const STRUCTURAL_IMPACT_SPEED: f32 = 10.0;
const SUPPORT_NORMAL_Y: f32 = 0.25;
const BLOCKED_DAMPING: f32 = 0.5;
const CONTACT_ITERATIONS: usize = 4;
const SUBSTEP_TRAVEL: f32 = 0.5;
const MAX_SUBSTEPS: u32 = 256;
const MAX_COMPONENT_RAW: i64 = 31 * fallingsand_math::SUBCELL_UNITS_PER_CELL as i64;

pub(super) struct PreparationDiagnostics {
    pub(super) reflood_us: u64,
    pub(super) derive_us: u64,
    pub(super) bodies_before: usize,
    pub(super) bodies_after_reflood: usize,
    pub(super) bodies_after_derive: usize,
    pub(super) flooded_bodies: usize,
    pub(super) split_bodies: usize,
    pub(super) split_fragments: usize,
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

#[derive(Default)]
pub(super) struct BodyStepper {
    entity_boxes: Vec<ActorAabb>,
    entity_states: Vec<PointState>,
    order: Vec<usize>,
    impulses: Vec<(f32, f32)>,
    raster: Raster,
    contact_raster: Raster,
    body_bins: BodyBins,
    body_chunks: Vec<Vec<ChunkPos>>,
    support_edges: Vec<Vec<usize>>,
    support_connected: Vec<bool>,
    support_queue: Vec<usize>,
    fluid_supported: Vec<bool>,
    orphan_cells: Vec<CellPos>,
    wake_targets: Vec<CellPos>,
    solver: SolverScratch,
}

impl BodyStepper {
    pub fn step<S>(
        &mut self,
        world: &mut CellWorld,
        bodies: &mut [PixelBody],
        entities: &[ActorDynamics],
        gravity: Subcell,
        simulated: &S,
        preparation: PreparationDiagnostics,
    ) -> &[(f32, f32)]
    where
        S: Fn(ChunkPos) -> bool,
    {
        let solver_start = Instant::now();
        let mut frozen_bodies = 0usize;
        let mut resting_skips = 0usize;
        let mut processed_bodies = 0usize;
        let mut substeps_total = 0u64;
        let mut contacts_total = 0u64;
        let mut vacated_total = 0usize;
        let mut wake_targets_total = 0usize;
        let mut motion_clamps = 0usize;
        let mut substep_clamps = 0usize;
        let mut requested_substeps_max = 0u32;
        let mut speed_max = 0.0f32;
        let mut spin_max = 0.0f32;
        let mut spin_body = 0u32;
        let mut spin_members = 0usize;
        let mut spin_inv_inertia = 0.0f32;
        let mut velocity_peak_raw_max = 0i64;
        let mut touching_substeps = 0u64;
        let mut supported_substeps = 0u64;
        let mut slow_substeps = 0u64;
        let mut rest_eligible_substeps = 0u64;
        self.orphan_cells.clear();
        self.entity_boxes.clear();
        self.entity_boxes
            .extend(entities.iter().map(|entity| entity.bbox));
        self.entity_states.clear();
        self.entity_states
            .extend(entities.iter().map(|entity| PointState {
                vx: entity.vx,
                vy: entity.vy,
                spin: 0.0,
                inv_mass: entity.inv_mass,
                inv_inertia: 0.0,
            }));

        self.order.clear();
        self.order.extend(0..bodies.len());
        self.order.sort_unstable_by_key(|&index| {
            (
                bodies[index].y.raw(),
                bodies[index].x.raw(),
                bodies[index].id,
            )
        });
        self.body_bins.clear();
        self.body_chunks.resize_with(bodies.len(), Vec::new);
        for (index, body) in bodies.iter().enumerate() {
            self.body_chunks[index].clear();
            index_body(
                &mut self.body_bins,
                &mut self.body_chunks[index],
                index,
                &body.raster,
            );
        }
        build_support_graph(
            world,
            bodies,
            &self.body_bins,
            &mut self.support_edges,
            &mut self.support_connected,
            &mut self.support_queue,
        );
        for &index in &self.order {
            let frozen = !span_simulated(world, simulated, &bodies[index]);
            bodies[index].frozen = frozen;
            if frozen {
                frozen_bodies += 1;
                continue;
            }
            if bodies[index].rest_secs > 0.0
                && !bodies[index].liquid_resting
                && bodies[index].vx == Subcell::ZERO
                && bodies[index].vy == Subcell::ZERO
                && bodies[index].spin == 0.0
            {
                resting_skips += 1;
                continue;
            }
            processed_bodies += 1;

            let (start_x, start_y, start_angle) = {
                let body = &bodies[index];
                (body.x, body.y, body.angle)
            };
            let substeps = {
                let body = &mut bodies[index];
                apply_buoyancy(world, body, gravity);
                body.vy += gravity;
                let (clamped, peak_raw) = clamp_body_motion(body);
                if clamped {
                    motion_clamps += 1;
                }
                velocity_peak_raw_max = velocity_peak_raw_max.max(peak_raw);
                let speed = body
                    .vx
                    .to_cells_per_second()
                    .hypot(body.vy.to_cells_per_second());
                speed_max = speed_max.max(speed);
                if body.spin.abs() > spin_max {
                    spin_max = body.spin.abs();
                    spin_body = body.id;
                    spin_members = body.raster.cells.len();
                    spin_inv_inertia = body.inv_inertia;
                }
                let travel = peak_raw as f32 / fallingsand_math::SUBCELL_UNITS_PER_CELL as f32;
                let requested = ((travel / SUBSTEP_TRAVEL).ceil() as u32).max(1);
                requested_substeps_max = requested_substeps_max.max(requested);
                if requested > MAX_SUBSTEPS {
                    substep_clamps += 1;
                }
                requested.min(MAX_SUBSTEPS)
            };
            substeps_total += u64::from(substeps);

            for _ in 0..substeps {
                let state = step_substep(
                    world,
                    bodies,
                    entities,
                    BodyMotion {
                        index,
                        substeps,
                        support_connected: self.support_connected[index],
                    },
                    &mut self.entity_states,
                    &mut self.solver,
                    ContactContext {
                        raster: &mut self.contact_raster,
                        bins: &self.body_bins,
                        orphan_cells: &mut self.orphan_cells,
                    },
                );
                touching_substeps += u64::from(state.touching);
                supported_substeps += u64::from(state.supported);
                slow_substeps += u64::from(state.slow);
                rest_eligible_substeps +=
                    u64::from(state.touching && state.supported && state.slow);
                contacts_total += self.solver.contacts.len() as u64;
            }
            let vacated = restamp(
                world,
                &self.entity_boxes,
                &mut bodies[index],
                start_x,
                start_y,
                start_angle,
                &mut self.raster,
            );
            vacated_total += vacated.len();
            append_vacated_wake_targets(
                &mut self.wake_targets,
                world,
                &|pos| bodies[index].raster.covers(pos),
                &vacated,
            );
            wake_targets_total += self.wake_targets.len();
            reindex_body(
                &mut self.body_bins,
                &mut self.body_chunks[index],
                index,
                &bodies[index].raster,
            );
            for &pos in &self.wake_targets {
                let Some(other) = self
                    .body_bins
                    .get(&pos.chunk())
                    .into_iter()
                    .flatten()
                    .copied()
                    .find(|&candidate| bodies[candidate].covers(pos))
                else {
                    continue;
                };
                if other != index {
                    bodies[other].rest_secs = 0.0;
                }
            }
            self.raster.clear();
        }

        build_support_graph(
            world,
            bodies,
            &self.body_bins,
            &mut self.support_edges,
            &mut self.support_connected,
            &mut self.support_queue,
        );
        let support_connected_bodies = self
            .support_connected
            .iter()
            .filter(|&&supported| supported)
            .count();
        build_fluid_support(
            world,
            bodies,
            &self.support_edges,
            &mut self.fluid_supported,
            &mut self.support_queue,
        );
        let fluid_supported_bodies = self
            .fluid_supported
            .iter()
            .filter(|&&supported| supported)
            .count();
        let rest_finalized_bodies =
            finalize_rest(bodies, &self.support_connected, &self.fluid_supported);

        for body in bodies.iter() {
            let _ = plan_and_commit(world, &self.entity_boxes, body, &body.raster);
        }

        self.impulses.clear();
        self.impulses.extend(
            entities
                .iter()
                .zip(&self.entity_states)
                .map(|(entity, state)| {
                    let mass = 1.0 / entity.inv_mass;
                    ((state.vx - entity.vx) * mass, (state.vy - entity.vy) * mass)
                }),
        );
        let solver_us = solver_start.elapsed().as_micros() as u64;
        let member_cells: usize = bodies.iter().map(|body| body.raster.cells.len()).sum();
        if !bodies.is_empty()
            || preparation.bodies_before != 0
            || preparation.reflood_us >= 1_000
            || preparation.derive_us >= 1_000
            || solver_us >= 1_000
        {
            tracing::info!(
                target: "body_diag",
                tick = world.tick(),
                reflood_us = preparation.reflood_us,
                derive_us = preparation.derive_us,
                solver_us,
                bodies_before = preparation.bodies_before,
                bodies_after_reflood = preparation.bodies_after_reflood,
                bodies_after_derive = preparation.bodies_after_derive,
                flooded_bodies = preparation.flooded_bodies,
                split_bodies = preparation.split_bodies,
                split_fragments = preparation.split_fragments,
                member_cells,
                processed_bodies,
                frozen_bodies,
                resting_skips,
                substeps_total,
                contacts_total,
                vacated_total,
                wake_targets_total,
                motion_clamps,
                substep_clamps,
                requested_substeps_max,
                speed_max,
                spin_max,
                spin_body,
                spin_members,
                spin_inv_inertia,
                velocity_peak_raw_max,
                touching_substeps,
                supported_substeps,
                slow_substeps,
                rest_eligible_substeps,
                support_connected_bodies,
                fluid_supported_bodies,
                rest_finalized_bodies,
                orphan_repairs = self.orphan_cells.len(),
                contact_chunks = self.body_bins.len(),
                "BODY SIM DIAGNOSTICS"
            );
        }
        &self.impulses
    }
}

fn clamp_body_motion(body: &mut PixelBody) -> (bool, i64) {
    let mut clamped = false;
    if body.inv_inertia == 0.0 && body.spin != 0.0 {
        body.spin = 0.0;
        clamped = true;
    }
    let Some((_, _, _, peak)) = rigid_velocity_field(body, &body.raster) else {
        return (clamped, 0);
    };
    if peak <= MAX_COMPONENT_RAW {
        return (clamped, peak);
    }
    let scale = MAX_COMPONENT_RAW as f32 / peak as f32;
    body.vx = body.vx.scaled_by(scale);
    body.vy = body.vy.scaled_by(scale);
    body.spin *= scale;
    let peak = rigid_velocity_field(body, &body.raster).map_or(0, |field| field.3);
    (true, peak)
}

fn index_body(bins: &mut BodyBins, chunks: &mut Vec<ChunkPos>, index: usize, raster: &Raster) {
    for &(pos, _) in &raster.cells {
        let chunk = pos.chunk();
        if chunks.contains(&chunk) {
            continue;
        }
        chunks.push(chunk);
        bins.entry(chunk).or_default().push(index);
    }
}

fn reindex_body(bins: &mut BodyBins, chunks: &mut Vec<ChunkPos>, index: usize, raster: &Raster) {
    for chunk in chunks.drain(..) {
        if let Some(indices) = bins.get_mut(&chunk) {
            indices.retain(|&candidate| candidate != index);
        }
    }
    index_body(bins, chunks, index, raster);
}

fn build_support_graph(
    world: &CellWorld,
    bodies: &[PixelBody],
    bins: &BodyBins,
    edges: &mut Vec<Vec<usize>>,
    connected: &mut Vec<bool>,
    queue: &mut Vec<usize>,
) {
    edges.resize_with(bodies.len(), Vec::new);
    for edge in edges.iter_mut() {
        edge.clear();
    }
    connected.clear();
    connected.resize(bodies.len(), false);
    queue.clear();

    for (index, body) in bodies.iter().enumerate() {
        for &(pos, _) in &body.raster.cells {
            for (dx, dy) in CARDINAL_NEIGHBORS {
                let neighbor = pos.translated(dx, dy);
                if body.raster.covers(neighbor) {
                    continue;
                }
                let other = bins
                    .get(&neighbor.chunk())
                    .into_iter()
                    .flatten()
                    .copied()
                    .find(|&candidate| candidate != index && bodies[candidate].covers(neighbor));
                if let Some(other) = other {
                    edges[index].push(other);
                    edges[other].push(index);
                    continue;
                }
                let terrain = world.get_cell(neighbor).is_none_or(|cell| {
                    !cell.is_body()
                        && matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
                });
                if terrain && !connected[index] {
                    connected[index] = true;
                    queue.push(index);
                }
            }
        }
    }

    let mut cursor = 0;
    while cursor < queue.len() {
        let support = queue[cursor];
        cursor += 1;
        for &body in &edges[support] {
            if !connected[body] {
                connected[body] = true;
                queue.push(body);
            }
        }
    }
}

fn build_fluid_support(
    world: &CellWorld,
    bodies: &[PixelBody],
    edges: &[Vec<usize>],
    supported: &mut Vec<bool>,
    queue: &mut Vec<usize>,
) {
    supported.clear();
    supported.resize(bodies.len(), false);
    queue.clear();
    for (index, body) in bodies.iter().enumerate() {
        if fluid_submersion(world, body) > 0.0 {
            supported[index] = true;
            queue.push(index);
        }
    }

    let mut cursor = 0;
    while cursor < queue.len() {
        let support = queue[cursor];
        cursor += 1;
        for &body in &edges[support] {
            if !supported[body] {
                supported[body] = true;
                queue.push(body);
            }
        }
    }
}

fn finalize_rest(
    bodies: &mut [PixelBody],
    support_connected: &[bool],
    fluid_supported: &[bool],
) -> usize {
    let mut finalized = 0;
    for ((body, &supported), &fluid_supported) in bodies
        .iter_mut()
        .zip(support_connected)
        .zip(fluid_supported)
    {
        if body.frozen {
            continue;
        }
        let vx = body.vx.to_cells_per_second();
        let vy = body.vy.to_cells_per_second();
        let slow = vx * vx + vy * vy < SETTLE_SPEED_SQ && body.spin.abs() < SETTLE_SPIN;
        if supported && slow {
            body.vx = Subcell::ZERO;
            body.vy = Subcell::ZERO;
            body.spin = 0.0;
            body.rest_secs += TICK_DT;
            body.liquid_resting = false;
            finalized += 1;
        } else if fluid_supported && slow {
            body.rest_secs += TICK_DT;
            body.liquid_resting = true;
            if body.rest_secs >= SETTLE_SECS {
                body.vx = Subcell::ZERO;
                body.vy = Subcell::ZERO;
                body.spin = 0.0;
            }
            finalized += 1;
        } else {
            body.rest_secs = 0.0;
            body.liquid_resting = false;
        }
    }
    finalized
}

fn fluid_submersion(world: &CellWorld, body: &PixelBody) -> f32 {
    let wet = body
        .perimeter
        .iter()
        .filter(|&&pos| {
            [(0, -1), (-1, 0), (1, 0)].into_iter().any(|(dx, dy)| {
                let neighbor = pos.translated(dx, dy);
                !body.raster.covers(neighbor)
                    && world
                        .get_cell(neighbor)
                        .is_some_and(|cell| content::phase(cell.material) == Phase::Liquid)
            })
        })
        .count();
    wet as f32 / body.perimeter.len().max(1) as f32
}

fn apply_buoyancy(world: &CellWorld, body: &mut PixelBody, gravity: Subcell) {
    let submersion = fluid_submersion(world, body);
    if submersion == 0.0 {
        return;
    }

    let mut up = Raster::default();
    let mut down = Raster::default();
    rasterize_into(&mut up, body, body.x, body.y.add_cells(1.0), body.angle);
    rasterize_into(&mut down, body, body.x, body.y.add_cells(-1.0), body.angle);
    if let (Some(up), Some(down)) = (
        displacement_potential(world, body, &up),
        displacement_potential(world, body, &down),
    ) {
        let displaced_mass = -(up - down) as f32 / (2.0 * REFERENCE_DENSITY_MILLI);
        body.vy -= gravity.scaled_by(displaced_mass * body.inv_mass);
    }

    if body.inv_inertia > 0.0 {
        let delta = std::f32::consts::TAU / body.angle_steps as f32;
        let mut counterclockwise = Raster::default();
        let mut clockwise = Raster::default();
        rasterize_into(
            &mut counterclockwise,
            body,
            body.x,
            body.y,
            body.angle + delta,
        );
        rasterize_into(&mut clockwise, body, body.x, body.y, body.angle - delta);
        if let (Some(counterclockwise), Some(clockwise)) = (
            displacement_potential(world, body, &counterclockwise),
            displacement_potential(world, body, &clockwise),
        ) {
            let torque =
                -(counterclockwise - clockwise) as f32 / (2.0 * delta * REFERENCE_DENSITY_MILLI);
            body.spin += torque * -gravity.to_cells_per_second() * body.inv_inertia;
        }
    }

    let speed = body
        .vx
        .to_cells_per_second()
        .hypot(body.vy.to_cells_per_second());
    let drag = fluid_drag(speed, submersion);
    body.vx = body.vx.scaled_by(1.0 - drag);
    body.vy = body.vy.scaled_by(1.0 - drag);
    body.spin *= 1.0 - drag;
}

fn displacement_potential(world: &CellWorld, body: &PixelBody, candidate: &Raster) -> Option<i128> {
    let mut displaced = Vec::new();
    for &(pos, _) in &candidate.cells {
        if body.raster.covers(pos) {
            continue;
        }
        let cell = world.get_cell(pos)?;
        if cell.is_body() || matches!(content::phase(cell.material), Phase::Solid | Phase::Powder) {
            return None;
        }
        if matches!(content::phase(cell.material), Phase::Liquid | Phase::Gas) {
            displaced.push((pos, cell));
        }
    }
    let mut vacated: Vec<_> = body
        .raster
        .set
        .iter()
        .filter(|pos| !candidate.covers(**pos))
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

    let mut claimed = rustc_hash::FxHashSet::default();
    let mut potential = 0i128;
    for (index, &(from, cell)) in displaced.iter().enumerate() {
        let target = if let Some(&target) = vacated.get(index) {
            target
        } else {
            let target = relocation_spot(world, &[], &claimed, &candidate.set, from)?;
            claimed.insert(target);
            target
        };
        potential +=
            i128::from(content::density_milli(cell.material)) * i128::from(target.y - from.y);
    }
    Some(potential)
}

#[derive(Clone, Copy)]
struct BodyMotion {
    index: usize,
    substeps: u32,
    support_connected: bool,
}

struct ContactContext<'a> {
    raster: &'a mut Raster,
    bins: &'a BodyBins,
    orphan_cells: &'a mut Vec<CellPos>,
}

struct SubstepDiagnostics {
    touching: bool,
    supported: bool,
    slow: bool,
}

fn step_substep(
    world: &mut CellWorld,
    bodies: &mut [PixelBody],
    entities: &[ActorDynamics],
    motion: BodyMotion,
    entity_states: &mut [PointState],
    scratch: &mut SolverScratch,
    contact: ContactContext<'_>,
) -> SubstepDiagnostics {
    let BodyMotion {
        index,
        substeps,
        support_connected,
    } = motion;
    let sub_dt = TICK_DT / substeps as f32;
    {
        let body = &mut bodies[index];
        body.x += body.vx.per_substep(substeps);
        body.y += body.vy.per_substep(substeps);
        body.angle = (body.angle + body.spin * sub_dt).rem_euclid(std::f32::consts::TAU);
    }

    rasterize_into(
        contact.raster,
        &bodies[index],
        bodies[index].x,
        bodies[index].y,
        bodies[index].angle,
    );
    find_contacts(
        &mut scratch.contacts,
        world,
        entities,
        bodies,
        contact.bins,
        index,
        contact.raster,
    );
    for obstruction in scratch
        .contacts
        .iter()
        .filter(|contact| contact.orphan)
        .map(|contact| contact.obstruction)
    {
        if contact.orphan_cells.contains(&obstruction) {
            continue;
        }
        contact.orphan_cells.push(obstruction);
        if let Some(mut cell) = world.get_cell(obstruction).filter(|cell| cell.is_body()) {
            cell.set_body(false);
            world.set_cell_raw(obstruction, cell);
        }
    }
    let touching = !scratch.contacts.is_empty();
    let supported = scratch.contacts.iter().any(|contact| {
        contact.ny > SUPPORT_NORMAL_Y
            && (contact.other.is_static()
                || support_connected && matches!(contact.other, Other::Body { .. }))
    });

    let restitution = bodies[index].restitution;
    scratch.points.clear();
    scratch.points.push(state_of(&bodies[index]));
    scratch.body_slots.clear();
    scratch.solver.clear();
    scratch.solver.reserve(scratch.contacts.len());
    for contact in &scratch.contacts {
        let partner = match contact.other {
            Other::Terrain => Partner::Static,
            Other::Body {
                index: other_index,
                rx,
                ry,
                ..
            } => {
                let slot = slot_for(
                    &mut scratch.points,
                    &mut scratch.body_slots,
                    other_index,
                    || state_of(&bodies[other_index]),
                );
                Partner::Body { slot, rx, ry }
            }
            Other::Entity {
                index: entity_index,
                ..
            } => Partner::Entity { slot: entity_index },
        };
        scratch.solver.push(SolverContact {
            rx: contact.rx,
            ry: contact.ry,
            nx: contact.nx,
            ny: contact.ny,
            restitution: restitution.max(contact.restitution),
            partner,
            bias: 0.0,
            acc_n: 0.0,
            acc_t: 0.0,
        });
    }

    for (contact, sc) in scratch.contacts.iter().zip(&mut scratch.solver) {
        let vn = relative_vn(&scratch.points, entity_states, sc);
        if -vn > STRUCTURAL_IMPACT_SPEED && matches!(contact.other, Other::Terrain) {
            world.note_terrain_interaction(contact.obstruction);
        }
        sc.bias = if -vn > BOUNCE_MIN_SPEED {
            sc.restitution * vn
        } else {
            0.0
        };
    }
    for _ in 0..CONTACT_ITERATIONS {
        for contact in 0..scratch.solver.len() {
            solve_contact(
                &mut scratch.solver,
                &mut scratch.points,
                entity_states,
                contact,
            );
        }
    }

    let active = scratch.points[0];
    let slow = active.vx * active.vx + active.vy * active.vy < SETTLE_SPEED_SQ
        && active.spin.abs() < SETTLE_SPIN;
    let body = &mut bodies[index];
    if touching && slow && supported {
        body.vx = Subcell::ZERO;
        body.vy = Subcell::ZERO;
        body.spin = 0.0;
    } else {
        body.vx = Subcell::from_cells_per_second(active.vx);
        body.vy = Subcell::from_cells_per_second(active.vy);
        body.spin = active.spin;
    }

    for &(other_index, slot) in &scratch.body_slots {
        let after = scratch.points[slot];
        let other = &mut bodies[other_index];
        other.vx = Subcell::from_cells_per_second(after.vx);
        other.vy = Subcell::from_cells_per_second(after.vy);
        other.spin = after.spin;
    }
    SubstepDiagnostics {
        touching,
        supported,
        slow,
    }
}

fn restamp(
    world: &mut CellWorld,
    entities: &[ActorAabb],
    body: &mut PixelBody,
    start_x: Subcell,
    start_y: Subcell,
    start_angle: f32,
    spare: &mut Raster,
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
        rasterize_into(spare, body, x, y, angle);
        if let Some(vacated) = plan_and_commit(world, entities, body, spare) {
            std::mem::swap(&mut body.raster, spare);
            body.pivot = body.raster.pivot.expect("body raster has a pivot");
            body.x = x;
            body.y = y;
            body.angle = angle;
            match attempt {
                1 => body.spin *= BLOCKED_DAMPING,
                2 => {
                    body.vx = body.vx.scaled_by(BLOCKED_DAMPING);
                    body.vy = body.vy.scaled_by(BLOCKED_DAMPING);
                }
                _ => {}
            }
            return vacated;
        }
    }

    body.x = start_x;
    body.y = start_y;
    body.angle = start_angle;
    body.vx = body.vx.scaled_by(BLOCKED_DAMPING);
    body.vy = body.vy.scaled_by(BLOCKED_DAMPING);
    body.spin *= BLOCKED_DAMPING;
    spare.clear();
    Vec::new()
}

fn plan_and_commit(
    world: &mut CellWorld,
    entities: &[ActorAabb],
    body: &PixelBody,
    new: &Raster,
) -> Option<Vec<CellPos>> {
    let (mut pivot_vx, mut pivot_vy, mut spin_raw, peak) = rigid_velocity_field(body, new)?;
    let pivot = new.pivot.expect("velocity field has a pivot");
    if peak > MAX_COMPONENT_RAW {
        let scale = MAX_COMPONENT_RAW as f64 / peak as f64;
        pivot_vx = (pivot_vx as f64 * scale).round() as i64;
        pivot_vy = (pivot_vy as f64 * scale).round() as i64;
        spin_raw = (spin_raw as f64 * scale).round() as i64;
    }

    let cell_for = |local: u16| {
        let pos = new
            .cells
            .iter()
            .find_map(|&(pos, index)| (index == local).then_some(pos))
            .expect("raster local index exists");
        let mut cell = body.cells[local as usize].cell;
        cell.set_vel(
            (pivot_vx - spin_raw * i64::from(pos.y - pivot.y)) as i32,
            (pivot_vy + spin_raw * i64::from(pos.x - pivot.x)) as i32,
        );
        cell.set_body(true);
        cell
    };
    commit_stamp(world, entities, &body.raster, new, &cell_for)
}

fn rigid_velocity_field(body: &PixelBody, raster: &Raster) -> Option<(i64, i64, i64, i64)> {
    let pivot = raster.pivot?;
    let mut mass = 0.0f32;
    let mut com_offset = (0.0f32, 0.0f32);
    for &(pos, local) in &raster.cells {
        let body_cell = &body.cells[local as usize];
        mass += body_cell.mass;
        com_offset.0 += body_cell.mass * (pos.x - pivot.x) as f32;
        com_offset.1 += body_cell.mass * (pos.y - pivot.y) as f32;
    }
    com_offset.0 /= mass;
    com_offset.1 /= mass;

    let spin_raw = Subcell::from_cells_per_second(body.spin).raw();
    let pivot_vx = body.vx.raw() + (spin_raw as f32 * com_offset.1).round() as i64;
    let pivot_vy = body.vy.raw() - (spin_raw as f32 * com_offset.0).round() as i64;
    let peak = raster
        .cells
        .iter()
        .map(|&(pos, _)| {
            (pivot_vx - spin_raw * i64::from(pos.y - pivot.y))
                .abs()
                .max((pivot_vy + spin_raw * i64::from(pos.x - pivot.x)).abs())
        })
        .max()
        .unwrap_or(0);
    Some((pivot_vx, pivot_vy, spin_raw, peak))
}

pub(super) fn settle_body(world: &mut CellWorld, body: &PixelBody) {
    settle_body_with(world, body, false);
}

pub(super) fn settle_body_quiet(world: &mut CellWorld, body: &PixelBody) {
    settle_body_with(world, body, true);
}

fn settle_body_with(world: &mut CellWorld, body: &PixelBody, quiet: bool) {
    for &(pos, _) in &body.raster.cells {
        let Some(mut cell) = world.get_cell(pos) else {
            continue;
        };
        cell.set_body(false);
        if quiet {
            world.set_cell_raw_quiet(pos, cell);
        } else {
            world.set_cell_raw(pos, cell);
        }
    }
}
