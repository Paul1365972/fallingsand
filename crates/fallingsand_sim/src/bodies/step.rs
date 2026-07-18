use super::contact::{Other, find_contacts};
use super::rotation::quantize_step;
use super::{
    ActorDynamics, OwnerMap, PixelBody, REFERENCE_DENSITY_MILLI, Raster, commit_stamp,
    rasterize_at, vacated_wake_targets, wake_covering,
};
use crate::physics::{ActorAabb, BOUNCE_MIN_SPEED, fluid_drag};
use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{CellPos, ChunkPos, Phase, Subcell, TICK_DT};

pub const SETTLE_SECS: f32 = 0.5;
const WAKE_SPEED: f32 = 0.5;
const SETTLE_SPEED_SQ: f32 = 100.0;
const SETTLE_SPIN: f32 = 1.5;
const SUPPORT_NORMAL_Y: f32 = 0.25;
const RESTING_SPIN_KEEP: f32 = 0.5;
const BLOCKED_DAMPING: f32 = 0.5;
const FRICTION: f32 = 0.4;
const CONTACT_ITERATIONS: usize = 4;
const PENETRATION_CORRECTION: f32 = 0.5;
const SUBSTEP_TRAVEL: f32 = 0.5;

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
    bodies: &mut [PixelBody],
    owners: &mut OwnerMap,
    entities: &[ActorDynamics],
    gravity: Subcell,
    simulated: &dyn Fn(ChunkPos) -> bool,
) -> Vec<(f32, f32)> {
    let entity_boxes: Vec<ActorAabb> = entities.iter().map(|entity| entity.bbox).collect();
    let mut entity_states: Vec<PointState> = entities
        .iter()
        .map(|entity| PointState {
            vx: entity.vx,
            vy: entity.vy,
            spin: 0.0,
            inv_mass: entity.inv_mass,
            inv_inertia: 0.0,
        })
        .collect();

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
        if frozen {
            continue;
        }
        {
            let body = &mut bodies[index];
            if body.rest_secs > 0.0
                && body.vx == Subcell::ZERO
                && body.vy == Subcell::ZERO
                && body.spin == 0.0
            {
                body.rest_secs += TICK_DT;
                continue;
            }
        }

        let (start_x, start_y, start_angle) = {
            let body = &bodies[index];
            (body.x, body.y, body.angle)
        };
        let substeps = {
            let body = &mut bodies[index];
            apply_buoyancy(world, body, gravity);
            body.vy += gravity;

            let radius = 0.5 * (body.width as f32).hypot(body.height as f32);
            let (vx, vy) = (body.vx.to_cells_per_second(), body.vy.to_cells_per_second());
            let travel = ((vx * vx + vy * vy).sqrt() + body.spin.abs() * radius) * TICK_DT;
            ((travel / SUBSTEP_TRAVEL).ceil() as u32).max(1)
        };

        for _ in 0..substeps {
            step_substep(
                world,
                bodies,
                owners,
                entities,
                index,
                substeps,
                &mut entity_states,
            );
        }
        let old_raster = bodies[index].raster.clone();
        let vacated = restamp(
            world,
            &entity_boxes,
            &mut bodies[index],
            start_x,
            start_y,
            start_angle,
        );
        owners.reseat(index, &old_raster, &bodies[index].raster);
        let targets =
            vacated_wake_targets(world, &|pos| bodies[index].raster.covers(pos), &vacated);
        for pos in targets {
            wake_covering(bodies, owners, pos);
        }
    }

    entities
        .iter()
        .zip(&entity_states)
        .map(|(entity, state)| {
            let mass = 1.0 / entity.inv_mass;
            ((state.vx - entity.vx) * mass, (state.vy - entity.vy) * mass)
        })
        .collect()
}

fn apply_buoyancy(world: &CellWorld, body: &mut PixelBody, gravity: Subcell) {
    const BEARING: [(i32, i32); 3] = [(0, -1), (-1, 0), (1, 0)];
    let step = quantize_step(body.angle, body.angle_steps);
    let pivot_cell = body.pivot_cell(body.x, body.y);
    let mut density_sum = 0.0f32;
    let mut samples = 0u32;
    let mut wet = 0u32;
    for &(lx, ly) in &body.perimeter {
        let pos = body.body_cell(pivot_cell, step, lx, ly);
        let mut bearing = false;
        for (dx, dy) in BEARING {
            let neighbor = pos.translated(dx, dy);
            if body.raster.covers(neighbor) {
                continue;
            }
            let Some(cell) = world.get_cell(neighbor) else {
                continue;
            };
            if content::phase(cell.material) == Phase::Liquid {
                density_sum += content::density_milli(cell.material) as f32;
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
    let buoyant =
        submersion * count as f32 * (density_sum / samples as f32) / REFERENCE_DENSITY_MILLI;
    body.vy -= gravity.scaled_by(buoyant * body.inv_mass);
    let speed = body
        .vx
        .to_cells_per_second()
        .hypot(body.vy.to_cells_per_second());
    let drag = fluid_drag(speed, submersion);
    body.vx = body.vx.scaled_by(1.0 - drag);
    body.vy = body.vy.scaled_by(1.0 - drag);
    body.spin *= 1.0 - drag;
}

#[derive(Clone, Copy)]
struct PointState {
    vx: f32,
    vy: f32,
    spin: f32,
    inv_mass: f32,
    inv_inertia: f32,
}

impl PointState {
    fn point_vel(&self, rx: f32, ry: f32) -> (f32, f32) {
        (self.vx - self.spin * ry, self.vy + self.spin * rx)
    }

    fn apply(&mut self, rx: f32, ry: f32, jx: f32, jy: f32) {
        self.vx += jx * self.inv_mass;
        self.vy += jy * self.inv_mass;
        self.spin += (rx * jy - ry * jx) * self.inv_inertia;
    }
}

enum Partner {
    Static,
    Body { slot: usize, rx: f32, ry: f32 },
    Entity { slot: usize },
}

struct SolverContact {
    rx: f32,
    ry: f32,
    nx: f32,
    ny: f32,
    restitution: f32,
    partner: Partner,
    bias: f32,
    acc_n: f32,
    acc_t: f32,
}

fn step_substep(
    world: &CellWorld,
    bodies: &mut [PixelBody],
    owners: &OwnerMap,
    entities: &[ActorDynamics],
    index: usize,
    substeps: u32,
    entity_states: &mut [PointState],
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

    let contacts = find_contacts(world, entities, bodies, owners, index);
    let touching = !contacts.is_empty();
    let restable = contacts
        .iter()
        .all(|contact| !matches!(contact.other, Other::Body { resting: false, .. }));
    let supported = contacts
        .iter()
        .any(|contact| contact.other.is_static() && contact.ny > SUPPORT_NORMAL_Y);

    let restitution = bodies[index].restitution;
    let mut points: Vec<PointState> = vec![state_of(&bodies[index])];
    let mut body_slots: Vec<(usize, usize)> = Vec::new();

    let mut solver: Vec<SolverContact> = Vec::with_capacity(contacts.len());
    for contact in &contacts {
        let partner = match contact.other {
            Other::Terrain => Partner::Static,
            Other::Body {
                index: other_index,
                rx,
                ry,
                ..
            } => {
                let slot = slot_for(&mut points, &mut body_slots, other_index, || {
                    state_of(&bodies[other_index])
                });
                Partner::Body { slot, rx, ry }
            }
            Other::Entity {
                index: entity_index,
                ..
            } => Partner::Entity { slot: entity_index },
        };
        solver.push(SolverContact {
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

    for sc in &mut solver {
        let vn = relative_vn(&points, entity_states, sc);
        sc.bias = if -vn > BOUNCE_MIN_SPEED {
            sc.restitution * vn
        } else {
            0.0
        };
    }

    for _ in 0..CONTACT_ITERATIONS {
        for i in 0..solver.len() {
            solve_contact(&mut solver, &mut points, entity_states, i);
        }
    }

    let active = points[0];
    let slow = active.vx * active.vx + active.vy * active.vy < SETTLE_SPEED_SQ
        && active.spin.abs() < SETTLE_SPIN;
    {
        let body = &mut bodies[index];
        if touching && slow && restable && supported {
            body.x = prev_x;
            body.y = prev_y;
            body.angle = prev_angle;
            body.vx = Subcell::ZERO;
            body.vy = Subcell::ZERO;
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
                body.x = body.x.add_cells(deepest.nx * correction);
                body.y = body.y.add_cells(deepest.ny * correction);
            }
            body.vx = Subcell::from_cells_per_second(active.vx);
            body.vy = Subcell::from_cells_per_second(active.vy);
            let spin = if touching && slow && supported {
                active.spin * RESTING_SPIN_KEEP
            } else {
                active.spin
            };
            body.spin = spin;
            body.rest_secs = 0.0;
        }
    }

    for &(other_index, slot) in &body_slots {
        let before = state_of(&bodies[other_index]);
        let after = points[slot];
        let other = &mut bodies[other_index];
        other.vx = Subcell::from_cells_per_second(after.vx);
        other.vy = Subcell::from_cells_per_second(after.vy);
        other.spin = after.spin;
        let moved = (after.vx - before.vx).abs() + (after.vy - before.vy).abs();
        if moved > WAKE_SPEED || (after.spin - before.spin).abs() > WAKE_SPEED {
            other.rest_secs = 0.0;
        }
    }
}

fn state_of(body: &PixelBody) -> PointState {
    PointState {
        vx: body.vx.to_cells_per_second(),
        vy: body.vy.to_cells_per_second(),
        spin: body.spin,
        inv_mass: body.inv_mass,
        inv_inertia: body.inv_inertia,
    }
}

fn slot_for(
    states: &mut Vec<PointState>,
    map: &mut Vec<(usize, usize)>,
    key: usize,
    make: impl FnOnce() -> PointState,
) -> usize {
    if let Some(&(_, slot)) = map.iter().find(|&&(k, _)| k == key) {
        return slot;
    }
    let slot = states.len();
    states.push(make());
    map.push((key, slot));
    slot
}

fn relative_vn(points: &[PointState], entities: &[PointState], sc: &SolverContact) -> f32 {
    let (ax, ay) = points[0].point_vel(sc.rx, sc.ry);
    let (bx, by) = partner_point_vel(points, entities, sc);
    (ax - bx) * sc.nx + (ay - by) * sc.ny
}

fn partner_point_vel(
    points: &[PointState],
    entities: &[PointState],
    sc: &SolverContact,
) -> (f32, f32) {
    match sc.partner {
        Partner::Static => (0.0, 0.0),
        Partner::Body { slot, rx, ry } => points[slot].point_vel(rx, ry),
        Partner::Entity { slot } => (entities[slot].vx, entities[slot].vy),
    }
}

fn partner_effective(
    points: &[PointState],
    entities: &[PointState],
    sc: &SolverContact,
) -> (f32, f32, f32, f32) {
    match sc.partner {
        Partner::Static => (0.0, 0.0, 0.0, 0.0),
        Partner::Body { slot, rx, ry } => (points[slot].inv_mass, points[slot].inv_inertia, rx, ry),
        Partner::Entity { slot } => (entities[slot].inv_mass, 0.0, 0.0, 0.0),
    }
}

fn apply_partner(
    points: &mut [PointState],
    entities: &mut [PointState],
    sc: &SolverContact,
    jx: f32,
    jy: f32,
) {
    match sc.partner {
        Partner::Static => {}
        Partner::Body { slot, rx, ry } => points[slot].apply(rx, ry, -jx, -jy),
        Partner::Entity { slot } => {
            entities[slot].vx -= jx * entities[slot].inv_mass;
            entities[slot].vy -= jy * entities[slot].inv_mass;
        }
    }
}

fn solve_contact(
    solver: &mut [SolverContact],
    points: &mut [PointState],
    entities: &mut [PointState],
    i: usize,
) {
    let (rx, ry, nx, ny, bias) = {
        let sc = &solver[i];
        (sc.rx, sc.ry, sc.nx, sc.ny, sc.bias)
    };
    let (other_inv_mass, other_inv_inertia, r2x, r2y) =
        partner_effective(points, entities, &solver[i]);

    let (ax, ay) = points[0].point_vel(rx, ry);
    let (bx, by) = partner_point_vel(points, entities, &solver[i]);
    let vn = (ax - bx) * nx + (ay - by) * ny;
    let r_cross_n = rx * ny - ry * nx;
    let r2_cross_n = r2x * ny - r2y * nx;
    let kn = points[0].inv_mass
        + other_inv_mass
        + r_cross_n * r_cross_n * points[0].inv_inertia
        + r2_cross_n * r2_cross_n * other_inv_inertia;
    let jn_target = -(vn + bias) / kn;
    let old_n = solver[i].acc_n;
    let new_n = (old_n + jn_target).max(0.0);
    let dn = new_n - old_n;
    solver[i].acc_n = new_n;
    points[0].apply(rx, ry, dn * nx, dn * ny);
    apply_partner(points, entities, &solver[i], dn * nx, dn * ny);

    let (tx, ty) = (-ny, nx);
    let (ax, ay) = points[0].point_vel(rx, ry);
    let (bx, by) = partner_point_vel(points, entities, &solver[i]);
    let vt = (ax - bx) * tx + (ay - by) * ty;
    let r_cross_t = rx * ty - ry * tx;
    let r2_cross_t = r2x * ty - r2y * tx;
    let kt = points[0].inv_mass
        + other_inv_mass
        + r_cross_t * r_cross_t * points[0].inv_inertia
        + r2_cross_t * r2_cross_t * other_inv_inertia;
    let jt_target = -vt / kt;
    let limit = FRICTION * solver[i].acc_n;
    let old_t = solver[i].acc_t;
    let new_t = (old_t + jt_target).clamp(-limit, limit);
    let dt = new_t - old_t;
    solver[i].acc_t = new_t;
    points[0].apply(rx, ry, dt * tx, dt * ty);
    apply_partner(points, entities, &solver[i], dt * tx, dt * ty);
}

fn restamp(
    world: &mut CellWorld,
    entities: &[ActorAabb],
    body: &mut PixelBody,
    start_x: Subcell,
    start_y: Subcell,
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
            plan_and_commit(world, entities, body, raster)
        };
        if let Some(vacated) = committed {
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
    Vec::new()
}

fn plan_and_commit(
    world: &mut CellWorld,
    entities: &[ActorAabb],
    body: &mut PixelBody,
    new: Raster,
) -> Option<Vec<CellPos>> {
    let cell_for = |local: u16| {
        let mut cell = body.cells[local as usize];
        cell.set_body(true);
        cell
    };
    let vacated = commit_stamp(world, entities, &body.raster, &new, &cell_for)?;
    body.raster = new;
    Some(vacated)
}

pub fn settle_body(world: &mut CellWorld, body: &PixelBody) {
    for &(pos, local) in &body.raster.cells {
        let mut cell = body.cells[local as usize];
        cell.set_body(false);
        world.set_cell_raw(pos, cell);
    }
}
