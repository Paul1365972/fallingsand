use super::contact::{Contact, Other, find_contacts};
use super::{
    ActorDynamics, PixelBody, REFERENCE_DENSITY, Raster, commit_stamp, quantized_trig,
    rasterize_at, relocation_spot, wake_covering,
};
use crate::physics::{
    ActorAabb, BOUNCE_MIN_SPEED, FLUID_DRAG_LINEAR, FLUID_DRAG_QUAD, MAX_FLUID_DRAG,
};
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, ChunkPos, Fixed, MaterialRegistry, Phase, TICK_DT};
use rustc_hash::FxHashSet;

const SLEEP_SECS: f32 = 0.33;
const WAKE_SPEED: f32 = 0.5;
const SETTLE_SPEED_SQ: f32 = 100.0;
const SETTLE_SPIN: f32 = 1.5;
const SUPPORT_NORMAL_Y: f32 = 0.25;
const CONTACT_KEEP_PER_SEC: f32 = 0.25;
const BLOCKED_DAMPING: f32 = 0.5;
const FRICTION: f32 = 0.4;
const CONTACT_ITERATIONS: usize = 4;
const PENETRATION_CORRECTION: f32 = 0.5;
const SUBSTEP_TRAVEL: f32 = 0.5;
const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

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
    entities: &[ActorDynamics],
    gravity: Fixed,
    simulated: &dyn Fn(ChunkPos) -> bool,
) -> Vec<(f32, f32)> {
    let mut entity_impulses = vec![(0.0, 0.0); entities.len()];
    let entity_boxes: Vec<ActorAabb> = entities.iter().map(|entity| entity.bbox).collect();

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
            body.vy = body.vy.add_vel_f32(gravity.to_f32() * TICK_DT);

            let radius = 0.5 * (body.width as f32).hypot(body.height as f32);
            let (vx, vy) = (body.vx.vel_f32(), body.vy.vel_f32());
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
        .add_vel_f32(-gravity.to_f32() * buoyant * body.inv_mass * TICK_DT);
    let speed = body.vx.vel_f32().hypot(body.vy.vel_f32());
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
    entities: &[ActorDynamics],
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
        let (mut vx, mut vy) = (body.vx.vel_f32(), body.vy.vel_f32());
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
            body.vx = Fixed::vel_per_sec(vx);
            body.vy = Fixed::vel_per_sec(vy);
            body.rest_secs = 0.0;
        }
    }

    for (other_index, jx, jy, r_cross_j) in body_impulses {
        let other = &mut bodies[other_index];
        let dvx = jx * other.inv_mass;
        let dvy = jy * other.inv_mass;
        let dspin = r_cross_j * other.inv_inertia;
        other.vx = other.vx.add_vel_f32(dvx);
        other.vy = other.vy.add_vel_f32(dvy);
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
    entities: &[ActorAabb],
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
    entities: &[ActorAabb],
    body: &mut PixelBody,
    new: Raster,
) -> Option<Vec<CellPos>> {
    let cell_for = |local: u16| {
        let mut cell = body.cells[local as usize];
        cell.set_body(true);
        cell
    };
    let vacated = commit_stamp(world, registry, entities, &body.raster, &new, &cell_for)?;
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
