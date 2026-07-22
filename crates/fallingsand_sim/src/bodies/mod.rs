mod island;
mod rotation;

pub use island::detect_detached_island;

use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, Phase, RegionPos, Subcell, content};
use fallingsand_math::SUBCELL_UNITS_PER_CELL;
use rotation::{ANGLE_STEPS, TURN_UNITS, quantize_step, rotate_offset, rotate_vector};
use rustc_hash::FxHashSet;

const MAX_TRAVEL_CELLS: i64 = 64;
const MAX_LINEAR_STEP: Subcell = Subcell::from_cell(MAX_TRAVEL_CELLS as i32);
const RESPONSE_SCALE: i128 = 1 << 16;
const MIN_RESTITUTION: u32 = RESPONSE_SCALE as u32 / 20;
const FRICTION: i128 = RESPONSE_SCALE / 4;
const SOLVER_SCALE: i128 = 1 << 16;
const TAU_NUMERATOR: i128 = 710;
const TAU_DENOMINATOR: i128 = 113;
const ANGULAR_DENOMINATOR: i128 = TURN_UNITS as i128 * TAU_DENOMINATOR;

#[derive(Default)]
pub struct BodySet {
    bodies: Vec<PixelBody>,
    current: Raster,
    candidate: Raster,
    constraints: Vec<Constraint>,
}

impl BodySet {
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    pub fn detach(&mut self, world: &mut CellWorld, island: Vec<CellPos>) {
        self.bodies.push(PixelBody::from_island(world, island));
    }

    pub fn push_at(&mut self, pos: CellPos, dvx: Subcell, dvy: Subcell, source_mass: u32) -> bool {
        let Some(body) = self.bodies.iter_mut().find(|body| body.raster.covers(pos)) else {
            return false;
        };
        let transferred_mass = body.mass.min(i64::from(source_mass));
        let jx = i128::from(dvx.raw()) * i128::from(transferred_mass);
        let jy = i128::from(dvy.raw()) * i128::from(transferred_mass);
        body.vx += Subcell::from_raw(round_div(jx, i128::from(body.mass)) as i64);
        body.vy += Subcell::from_raw(round_div(jy, i128::from(body.mass)) as i64);
        if body.inertia != 0 {
            let rx = i128::from(Subcell::cell_center(pos.x).raw() - body.x.raw());
            let ry = i128::from(Subcell::cell_center(pos.y).raw() - body.y.raw());
            let torque = rx * jy - ry * jx;
            body.angular_velocity += round_div(
                torque * i128::from(TURN_UNITS) * TAU_DENOMINATOR,
                body.inertia * TAU_NUMERATOR,
            ) as i64;
        }
        true
    }

    pub fn rasters(&self) -> impl Iterator<Item = impl Iterator<Item = CellPos> + '_> + '_ {
        self.bodies
            .iter()
            .map(|body| body.raster.cells.iter().map(|&(pos, _)| pos))
    }

    pub fn step(
        &mut self,
        world: &mut CellWorld,
        gravity: Subcell,
        simulated: impl Fn(fallingsand_core::ChunkPos) -> bool,
    ) {
        self.bodies.sort_unstable_by_key(PixelBody::bottom);
        let mut index = 0;
        while index < self.bodies.len() {
            let outcome = step_body(
                world,
                &mut self.bodies[index],
                gravity,
                &simulated,
                &mut self.current,
                &mut self.candidate,
                &mut self.constraints,
            );
            if outcome == StepOutcome::Settled {
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
            let overlaps = self.bodies[index].raster.cells.iter().any(|&(cell, _)| {
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

struct PixelBody {
    cells: Vec<BodyCell>,
    raster: Raster,
    x: Subcell,
    y: Subcell,
    vx: Subcell,
    vy: Subcell,
    angle: i64,
    angular_velocity: i64,
    radius: i64,
    mass: i64,
    inertia: i128,
    restitution: u32,
}

impl PixelBody {
    fn from_island(world: &mut CellWorld, island: Vec<CellPos>) -> Self {
        let mass = island.len() as i64;
        let sum_x2: i64 = island.iter().map(|cell| i64::from(cell.x) * 2 + 1).sum();
        let sum_y2: i64 = island.iter().map(|cell| i64::from(cell.y) * 2 + 1).sum();
        let units = SUBCELL_UNITS_PER_CELL as i128;
        let x =
            Subcell::from_raw(round_div(i128::from(sum_x2) * units, i128::from(mass) * 2) as i64);
        let y =
            Subcell::from_raw(round_div(i128::from(sum_y2) * units, i128::from(mass) * 2) as i64);
        let pivot = island
            .iter()
            .copied()
            .min_by_key(|cell| {
                let dx = (i64::from(cell.x) * 2 + 1) * mass - sum_x2;
                let dy = (i64::from(cell.y) * 2 + 1) * mass - sum_y2;
                (
                    i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy),
                    cell.y,
                    cell.x,
                )
            })
            .expect("body island is not empty");
        let cells: Vec<_> = island
            .iter()
            .map(|&pos| {
                let mut cell = world.get_cell(pos).expect("body island is loaded");
                cell.set_body(true);
                BodyCell {
                    cell,
                    local: (pos.x - pivot.x, pos.y - pivot.y),
                }
            })
            .collect();
        let radius = cells
            .iter()
            .map(|cell| i64::from(cell.local.0.abs() + cell.local.1.abs()) + 1)
            .max()
            .unwrap_or(1);
        let inertia = island
            .iter()
            .map(|cell| {
                let rx = i128::from(Subcell::cell_center(cell.x).raw() - x.raw());
                let ry = i128::from(Subcell::cell_center(cell.y).raw() - y.raw());
                rx * rx + ry * ry
            })
            .sum();
        let restitution = cells
            .iter()
            .map(|cell| content::restitution_q16(cell.cell.material))
            .max()
            .unwrap_or(0)
            .max(MIN_RESTITUTION);
        let mut body = Self {
            cells,
            raster: Raster::default(),
            x,
            y,
            vx: Subcell::ZERO,
            vy: Subcell::ZERO,
            angle: 0,
            angular_velocity: 0,
            radius,
            mass,
            inertia,
            restitution,
        };
        rasterize(&mut body.raster, &body.cells, body.x, body.y, 0);
        for &(pos, local) in &body.raster.cells {
            world.set_cell_raw(pos, body.cells[local as usize].cell);
        }
        body
    }

    fn bottom(&self) -> i32 {
        self.raster
            .cells
            .iter()
            .map(|(cell, _)| cell.y)
            .min()
            .unwrap_or(i32::MIN)
    }
}

struct BodyCell {
    cell: Cell,
    local: (i32, i32),
}

#[derive(Clone, Default)]
struct Raster {
    cells: Vec<(CellPos, u16)>,
    set: FxHashSet<CellPos>,
}

impl Raster {
    fn covers(&self, pos: CellPos) -> bool {
        self.set.contains(&pos)
    }

    fn clear(&mut self) {
        self.cells.clear();
        self.set.clear();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepOutcome {
    Active,
    Settled,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Pose {
    x: Subcell,
    y: Subcell,
    angle: i64,
}

#[derive(Clone, Copy)]
struct Transition {
    from: Pose,
    to: Pose,
}

#[derive(Clone, Copy)]
struct BlockedState {
    pose: Pose,
    before_force: Motion,
    gravity: Subcell,
}

#[derive(Clone, Copy)]
struct Pivot {
    start: Pose,
    point: (i64, i64),
    angular: i64,
}

fn traversal_steps(body: &PixelBody, gravity: Subcell) -> u32 {
    let end_vy = (body.vy + gravity).clamp(-MAX_LINEAR_STEP, MAX_LINEAR_STEP);
    motion_traversal_steps(
        body,
        Motion {
            x: body.vx.raw(),
            y: body.vy.raw().abs().max(end_vy.raw().abs()),
            angular: body.angular_velocity,
        },
    )
}

fn motion_traversal_steps(body: &PixelBody, motion: Motion) -> u32 {
    let linear = i128::from(motion.x.abs()) + i128::from(motion.y.abs());
    let angular = i128::from(motion.angular.abs())
        * TAU_NUMERATOR
        * i128::from(body.radius)
        * i128::from(SUBCELL_UNITS_PER_CELL);
    let travel_steps = ceil_div(
        linear * ANGULAR_DENOMINATOR + angular,
        i128::from(SUBCELL_UNITS_PER_CELL) * ANGULAR_DENOMINATOR,
    );
    let orientation_steps = ceil_div(
        i128::from(motion.angular.abs()) * i128::from(ANGLE_STEPS),
        i128::from(TURN_UNITS),
    );
    travel_steps.max(orientation_steps).max(1) as u32
}

fn step_body(
    world: &mut CellWorld,
    body: &mut PixelBody,
    gravity: Subcell,
    simulated: &impl Fn(fallingsand_core::ChunkPos) -> bool,
    current: &mut Raster,
    candidate: &mut Raster,
    constraints: &mut Vec<Constraint>,
) -> StepOutcome {
    let start_pose = Pose {
        x: body.x,
        y: body.y,
        angle: body.angle,
    };
    body.vx = body.vx.clamp(-MAX_LINEAR_STEP, MAX_LINEAR_STEP);
    body.vy = body.vy.clamp(-MAX_LINEAR_STEP, MAX_LINEAR_STEP);
    let max_angular_velocity = round_div(
        i128::from(MAX_TRAVEL_CELLS * TURN_UNITS) * TAU_DENOMINATOR,
        i128::from(body.radius) * TAU_NUMERATOR,
    )
    .min(i128::from(TURN_UNITS)) as i64;
    body.angular_velocity = body
        .angular_velocity
        .clamp(-max_angular_velocity, max_angular_velocity);
    let substeps = traversal_steps(body, gravity);
    let mut pose = start_pose;
    current.clone_from(&body.raster);
    let mut x_remainder = 0;
    let mut y_remainder = 0;
    let mut angle_remainder = 0;
    let mut gravity_remainder = 0;
    let mut had_contact = false;

    for _ in 0..substeps {
        let before_force = Motion::from_body(body);
        body.vy += Subcell::from_raw(split_step(&mut gravity_remainder, gravity.raw(), substeps));
        let dx = Subcell::from_raw(split_step(&mut x_remainder, body.vx.raw(), substeps));
        let dy = Subcell::from_raw(split_step(&mut y_remainder, body.vy.raw(), substeps));
        let da = split_step(&mut angle_remainder, body.angular_velocity, substeps);
        if dx == Subcell::ZERO && dy == Subcell::ZERO && da == 0 {
            continue;
        }
        let next = Pose {
            x: pose.x + dx,
            y: pose.y + dy,
            angle: pose.angle + da,
        };
        match proposal(
            world,
            body,
            current,
            candidate,
            Transition {
                from: pose,
                to: next,
            },
            simulated,
            constraints,
        ) {
            Proposal::Free => {
                pose = next;
                std::mem::swap(current, candidate);
                candidate.clear();
            }
            Proposal::Blocked => {
                had_contact = true;
                compact_constraints(constraints);
                let response = resolve_constraints(
                    world,
                    body,
                    current,
                    candidate,
                    constraints,
                    BlockedState {
                        pose,
                        before_force,
                        gravity,
                    },
                    simulated,
                );
                response.motion.assign(body);
                if let Some(successor) = response.successor {
                    pose = successor;
                    rasterize(candidate, &body.cells, pose.x, pose.y, pose.angle);
                    std::mem::swap(current, candidate);
                }
                candidate.clear();
                break;
            }
            Proposal::Frozen => {
                freeze(body, current, candidate);
                return StepOutcome::Active;
            }
        }
    }

    let moved = pose != start_pose;
    if moved {
        if current.cells != body.raster.cells {
            commit(world, body, current);
            std::mem::swap(&mut body.raster, current);
        }
        body.x = pose.x;
        body.y = pose.y;
        body.angle = pose.angle.rem_euclid(TURN_UNITS);
    }
    current.clear();
    candidate.clear();
    if had_contact
        && body.vx == Subcell::ZERO
        && body.vy == Subcell::ZERO
        && body.angular_velocity == 0
    {
        StepOutcome::Settled
    } else {
        StepOutcome::Active
    }
}

fn freeze(body: &mut PixelBody, current: &mut Raster, candidate: &mut Raster) {
    body.vx = Subcell::ZERO;
    body.vy = Subcell::ZERO;
    body.angular_velocity = 0;
    current.clear();
    candidate.clear();
}

fn proposal(
    world: &CellWorld,
    body: &PixelBody,
    current: &Raster,
    candidate: &mut Raster,
    transition: Transition,
    simulated: &impl Fn(fallingsand_core::ChunkPos) -> bool,
    constraints: &mut Vec<Constraint>,
) -> Proposal {
    rasterize(
        candidate,
        &body.cells,
        transition.to.x,
        transition.to.y,
        transition.to.angle,
    );
    classify(
        world,
        &body.raster,
        current,
        candidate,
        transition.from,
        simulated,
        constraints,
    )
}

enum Proposal {
    Free,
    Blocked,
    Frozen,
}

#[derive(Clone, Copy)]
struct Constraint {
    nx: i64,
    ny: i64,
    lever: i128,
    tangent_lever: i128,
    restitution: u32,
}

fn classify(
    world: &CellWorld,
    owned: &Raster,
    current: &Raster,
    candidate: &Raster,
    pose: Pose,
    simulated: &impl Fn(fallingsand_core::ChunkPos) -> bool,
    constraints: &mut Vec<Constraint>,
) -> Proposal {
    constraints.clear();
    for &(pos, local) in &candidate.cells {
        if current.covers(pos) {
            continue;
        }
        if !simulated(pos.chunk()) {
            return Proposal::Frozen;
        }
        let Some(cell) = world.get_cell(pos) else {
            return Proposal::Frozen;
        };
        if !owned.covers(pos)
            && (cell.is_body()
                || matches!(content::phase(cell.material), Phase::Solid | Phase::Powder))
        {
            let previous = current.cells[local as usize].0;
            let dx = i64::from(pos.x - previous.x);
            let dy = i64::from(pos.y - previous.y);
            let (nx, ny) = if dx.abs() >= dy.abs() && dx != 0 {
                (-dx.signum(), 0)
            } else if dy != 0 {
                (0, -dy.signum())
            } else {
                continue;
            };
            let x =
                (Subcell::cell_center(previous.x).raw() + Subcell::cell_center(pos.x).raw()) / 2;
            let y =
                (Subcell::cell_center(previous.y).raw() + Subcell::cell_center(pos.y).raw()) / 2;
            let rx = i128::from(x - pose.x.raw());
            let ry = i128::from(y - pose.y.raw());
            constraints.push(Constraint {
                nx,
                ny,
                lever: rx * i128::from(ny) - ry * i128::from(nx),
                tangent_lever: rx * i128::from(nx) + ry * i128::from(ny),
                restitution: content::restitution_q16(cell.material),
            });
        }
    }
    if constraints.is_empty() {
        Proposal::Free
    } else {
        Proposal::Blocked
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Motion {
    x: i64,
    y: i64,
    angular: i64,
}

impl Motion {
    const ZERO: Self = Self {
        x: 0,
        y: 0,
        angular: 0,
    };

    fn from_body(body: &PixelBody) -> Self {
        Self {
            x: body.vx.raw(),
            y: body.vy.raw(),
            angular: body.angular_velocity,
        }
    }

    fn assign(self, body: &mut PixelBody) {
        body.vx = Subcell::from_raw(self.x);
        body.vy = Subcell::from_raw(self.y);
        body.angular_velocity = self.angular;
    }
}

#[derive(Clone, Copy)]
struct PreciseMotion {
    x: i128,
    y: i128,
    angular: i128,
}

impl PreciseMotion {
    fn from_motion(motion: Motion) -> Self {
        Self {
            x: ANGULAR_DENOMINATOR * i128::from(motion.x) * SOLVER_SCALE,
            y: ANGULAR_DENOMINATOR * i128::from(motion.y) * SOLVER_SCALE,
            angular: TAU_NUMERATOR * i128::from(motion.angular) * SOLVER_SCALE,
        }
    }
}

#[derive(Clone, Copy)]
struct ConstraintRow {
    x: i128,
    y: i128,
    angular: i128,
}

impl Constraint {
    fn normal(self) -> ConstraintRow {
        ConstraintRow {
            x: i128::from(self.nx),
            y: i128::from(self.ny),
            angular: self.lever,
        }
    }

    fn tangent(self) -> ConstraintRow {
        ConstraintRow {
            x: i128::from(-self.ny),
            y: i128::from(self.nx),
            angular: self.tangent_lever,
        }
    }

    fn point(self) -> (i64, i64) {
        (
            clamp_i128(self.tangent_lever * i128::from(self.nx) + self.lever * i128::from(self.ny)),
            clamp_i128(self.tangent_lever * i128::from(self.ny) - self.lever * i128::from(self.nx)),
        )
    }
}

fn compact_constraints(constraints: &mut Vec<Constraint>) {
    let mut extremes: [[Option<Constraint>; 2]; 4] = [[None; 2]; 4];
    let mut restitution = [0; 4];
    for &constraint in constraints.iter() {
        let direction = match (constraint.nx, constraint.ny) {
            (1, 0) => 0,
            (-1, 0) => 1,
            (0, 1) => 2,
            (0, -1) => 3,
            _ => unreachable!("raster constraint normal is cardinal"),
        };
        restitution[direction] = restitution[direction].max(constraint.restitution);
        match extremes[direction][0] {
            Some(current) if current.lever < constraint.lever => {}
            _ => extremes[direction][0] = Some(constraint),
        }
        match extremes[direction][1] {
            Some(current) if current.lever > constraint.lever => {}
            _ => extremes[direction][1] = Some(constraint),
        }
    }
    constraints.clear();
    for (direction, [minimum, maximum]) in extremes.into_iter().enumerate() {
        if let Some(mut minimum) = minimum {
            minimum.restitution = restitution[direction];
            constraints.push(minimum);
        }
        if let Some(mut maximum) = maximum
            && minimum.is_none_or(|minimum| minimum.lever != maximum.lever)
        {
            maximum.restitution = restitution[direction];
            constraints.push(maximum);
        }
    }
}

struct Response {
    motion: Motion,
    successor: Option<Pose>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ResponsePriority {
    Support,
    SharedFace,
    Impact,
    Stop,
}

struct ResponseSearch<'a> {
    world: &'a CellWorld,
    body: &'a PixelBody,
    current: &'a Raster,
    candidate: &'a mut Raster,
    simulated: &'a dyn Fn(fallingsand_core::ChunkPos) -> bool,
    blocked: BlockedState,
    desired: Motion,
    best: (ResponsePriority, i128, Response),
}

impl ResponseSearch<'_> {
    fn consider(&mut self, motion: Motion) {
        self.consider_with(ResponsePriority::Impact, motion);
    }

    fn consider_with(&mut self, mut priority: ResponsePriority, motion: Motion) {
        if motion == Motion::ZERO {
            return;
        }
        let Some(successor) = legal_successor(
            self.world,
            self.body,
            self.current,
            self.candidate,
            self.blocked.pose,
            motion,
            self.simulated,
        ) else {
            return;
        };
        if priority == ResponsePriority::SharedFace
            || (motion.x == 0 && motion.y == 0 && motion.angular != 0)
        {
            rasterize(
                self.candidate,
                &self.body.cells,
                successor.x,
                successor.y,
                successor.angle,
            );
            if self.candidate.cells == self.current.cells {
                if priority != ResponsePriority::SharedFace {
                    self.candidate.clear();
                    return;
                }
                priority = ResponsePriority::Impact;
            }
            self.candidate.clear();
        }
        self.accept(priority, motion, successor);
    }

    fn consider_pivot(&mut self, angular: i64, constraint: Constraint, sustained: bool) {
        if angular == 0 {
            return;
        }
        let Some((successor, point)) = legal_pivot_successor(
            self.world,
            self.body,
            self.current,
            self.candidate,
            Pivot {
                start: self.blocked.pose,
                point: constraint.point(),
                angular,
            },
            self.simulated,
        ) else {
            return;
        };
        let motion = pivot_motion(angular, point);
        let priority = if sustained {
            ResponsePriority::Support
        } else {
            ResponsePriority::Impact
        };
        self.accept(priority, motion, successor);
    }

    fn accept(&mut self, priority: ResponsePriority, motion: Motion, successor: Pose) {
        let energy_budget = kinetic_energy(self.body, self.blocked.before_force)
            + 2 * i128::from(self.body.mass)
                * i128::from(self.blocked.gravity.raw())
                * i128::from(successor.y.raw() - self.blocked.pose.y.raw())
                * ANGULAR_DENOMINATOR
                * ANGULAR_DENOMINATOR;
        if kinetic_energy(self.body, motion) > energy_budget {
            return;
        }
        let score = motion_distance(self.body, motion, self.desired);
        if (priority, score) < (self.best.0, self.best.1) {
            self.best = (
                priority,
                score,
                Response {
                    motion,
                    successor: Some(successor),
                },
            );
        }
    }
}

fn resolve_constraints(
    world: &CellWorld,
    body: &mut PixelBody,
    current: &Raster,
    candidate: &mut Raster,
    constraints: &mut [Constraint],
    blocked: BlockedState,
    simulated: &impl Fn(fallingsand_core::ChunkPos) -> bool,
) -> Response {
    if body.inertia == 0 {
        for constraint in constraints.iter_mut() {
            constraint.lever = 0;
            constraint.tangent_lever = 0;
        }
    }
    let desired = Motion::from_body(body);
    let mut search = ResponseSearch {
        world,
        body,
        current,
        candidate,
        simulated,
        blocked,
        desired,
        best: (
            ResponsePriority::Stop,
            motion_distance(body, Motion::ZERO, desired),
            Response {
                motion: Motion::ZERO,
                successor: None,
            },
        ),
    };
    let sustained = constraints
        .iter()
        .all(|&constraint| constraint_velocity(blocked.before_force, constraint) >= 0);
    if !sustained {
        search.consider(blocked.before_force);
    }
    for &constraint in constraints.iter() {
        for source in [desired, blocked.before_force] {
            let inelastic = nearest_motion(project_plane(
                body,
                PreciseMotion::from_motion(source),
                constraint.normal(),
            ));
            let inelastic = apply_friction(body, inelastic, constraint);
            search.consider(inelastic);
            search.consider(separate_from_plane(inelastic, constraint));
            search.consider_pivot(
                inelastic.angular,
                constraint,
                sustained && blocked.gravity.raw() * constraint.ny < 0,
            );
            if constraint_velocity(blocked.before_force, constraint) < 0 {
                let restitution = body.restitution.max(constraint.restitution);
                let reflected = Motion {
                    x: inelastic.x
                        + round_div(
                            i128::from(inelastic.x - source.x) * i128::from(restitution),
                            RESPONSE_SCALE,
                        ) as i64,
                    y: inelastic.y
                        + round_div(
                            i128::from(inelastic.y - source.y) * i128::from(restitution),
                            RESPONSE_SCALE,
                        ) as i64,
                    angular: inelastic.angular
                        + round_div(
                            i128::from(inelastic.angular - source.angular)
                                * i128::from(restitution),
                            RESPONSE_SCALE,
                        ) as i64,
                };
                let reflected = apply_friction(body, reflected, constraint);
                search.consider(reflected);
                search.consider(separate_from_plane(reflected, constraint));
            }
        }
    }
    for pair in constraints.windows(2) {
        let [first, second] = pair else {
            unreachable!()
        };
        if (first.nx, first.ny) != (second.nx, second.ny) {
            continue;
        }
        for source in [desired, blocked.before_force] {
            search.consider(face_response(source, *first, 0));
            if constraint_velocity(blocked.before_force, *first) < 0
                || constraint_velocity(blocked.before_force, *second) < 0
            {
                let restitution = body
                    .restitution
                    .max(first.restitution)
                    .max(second.restitution);
                let reflected = face_response(source, *first, restitution);
                search.consider_with(ResponsePriority::SharedFace, reflected);
            }
        }
    }
    search.best.2
}

fn face_response(source: Motion, constraint: Constraint, restitution: u32) -> Motion {
    let normal = source.x * constraint.nx + source.y * constraint.ny;
    let tangent = (-source.x * constraint.ny + source.y * constraint.nx)
        * (RESPONSE_SCALE - FRICTION) as i64
        / RESPONSE_SCALE as i64;
    let normal = if normal < 0 {
        round_div(
            -i128::from(normal) * i128::from(restitution),
            RESPONSE_SCALE,
        ) as i64
    } else {
        normal
    };
    Motion {
        x: normal * constraint.nx - tangent * constraint.ny,
        y: normal * constraint.ny + tangent * constraint.nx,
        angular: 0,
    }
}

fn separate_from_plane(mut motion: Motion, constraint: Constraint) -> Motion {
    let inward = motion.x * constraint.nx + motion.y * constraint.ny;
    if inward < 0 {
        motion.x -= inward * constraint.nx;
        motion.y -= inward * constraint.ny;
    }
    motion
}

fn apply_friction(body: &PixelBody, motion: Motion, constraint: Constraint) -> Motion {
    let tangent = nearest_motion(project_plane(
        body,
        PreciseMotion::from_motion(motion),
        constraint.tangent(),
    ));
    Motion {
        x: damp_toward(motion.x, tangent.x),
        y: damp_toward(motion.y, tangent.y),
        angular: damp_toward(motion.angular, tangent.angular),
    }
}

fn damp_toward(value: i64, target: i64) -> i64 {
    let delta = target - value;
    let change = round_div(i128::from(delta) * FRICTION, RESPONSE_SCALE) as i64;
    value + if change == 0 { delta.signum() } else { change }
}

fn legal_successor(
    world: &CellWorld,
    body: &PixelBody,
    current: &Raster,
    candidate: &mut Raster,
    mut pose: Pose,
    motion: Motion,
    simulated: &dyn Fn(fallingsand_core::ChunkPos) -> bool,
) -> Option<Pose> {
    let start = pose;
    let steps = motion_traversal_steps(body, motion);
    let mut x_remainder = 0;
    let mut y_remainder = 0;
    let mut angle_remainder = 0;
    for _ in 0..steps {
        let next = Pose {
            x: pose.x + Subcell::from_raw(split_step(&mut x_remainder, motion.x, steps)),
            y: pose.y + Subcell::from_raw(split_step(&mut y_remainder, motion.y, steps)),
            angle: pose.angle + split_step(&mut angle_remainder, motion.angular, steps),
        };
        if next == pose {
            continue;
        }
        rasterize(candidate, &body.cells, next.x, next.y, next.angle);
        if candidate.cells == current.cells {
            pose = next;
            continue;
        }
        let free = raster_is_free(world, body, current, candidate, simulated);
        candidate.clear();
        return free.then_some(next);
    }
    candidate.clear();
    (pose != start).then_some(pose)
}

fn legal_pivot_successor(
    world: &CellWorld,
    body: &PixelBody,
    current: &Raster,
    candidate: &mut Raster,
    pivot: Pivot,
    simulated: &dyn Fn(fallingsand_core::ChunkPos) -> bool,
) -> Option<(Pose, (i64, i64))> {
    let steps = motion_traversal_steps(
        body,
        Motion {
            x: 0,
            y: 0,
            angular: pivot.angular,
        },
    );
    let mut angle_remainder = 0;
    let mut delta = 0;
    let mut pose = pivot.start;
    let mut rotated = pivot.point;
    for _ in 0..steps {
        delta += split_step(&mut angle_remainder, pivot.angular, steps);
        rotated = rotate_phase(delta, pivot.point);
        let next = Pose {
            x: pivot.start.x + Subcell::from_raw(pivot.point.0 - rotated.0),
            y: pivot.start.y + Subcell::from_raw(pivot.point.1 - rotated.1),
            angle: pivot.start.angle + delta,
        };
        rasterize(candidate, &body.cells, next.x, next.y, next.angle);
        if candidate.cells == current.cells {
            pose = next;
            continue;
        }
        let free = raster_is_free(world, body, current, candidate, simulated);
        candidate.clear();
        return free.then_some((next, rotated));
    }
    candidate.clear();
    (pose != pivot.start
        && pivot_boundary_is_free(world, body, current, candidate, pivot, simulated))
    .then_some((pose, rotated))
}

fn pivot_boundary_is_free(
    world: &CellWorld,
    body: &PixelBody,
    current: &Raster,
    candidate: &mut Raster,
    pivot: Pivot,
    simulated: &dyn Fn(fallingsand_core::ChunkPos) -> bool,
) -> bool {
    let step = pivot.angular.signum();
    let current_step = quantize_step(pivot.start.angle);
    let mut low = 1;
    let mut high = TURN_UNITS / i64::from(ANGLE_STEPS) + 1;
    while low < high {
        let middle = (low + high) / 2;
        if quantize_step(pivot.start.angle + step * middle) == current_step {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let delta = step * low;
    let rotated = rotate_phase(delta, pivot.point);
    let boundary = Pose {
        x: pivot.start.x + Subcell::from_raw(pivot.point.0 - rotated.0),
        y: pivot.start.y + Subcell::from_raw(pivot.point.1 - rotated.1),
        angle: pivot.start.angle + delta,
    };
    rasterize(
        candidate,
        &body.cells,
        boundary.x,
        boundary.y,
        boundary.angle,
    );
    let free = candidate.cells != current.cells
        && raster_is_free(world, body, current, candidate, simulated);
    candidate.clear();
    free
}

fn raster_is_free(
    world: &CellWorld,
    body: &PixelBody,
    current: &Raster,
    candidate: &Raster,
    simulated: &dyn Fn(fallingsand_core::ChunkPos) -> bool,
) -> bool {
    candidate.cells.iter().all(|&(pos, _)| {
        if current.covers(pos) || body.raster.covers(pos) {
            return true;
        }
        simulated(pos.chunk())
            && world.get_cell(pos).is_some_and(|cell| {
                !cell.is_body()
                    && !matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
            })
    })
}

fn rotate_phase(angle: i64, point: (i64, i64)) -> (i64, i64) {
    let scaled = i128::from(angle.rem_euclid(TURN_UNITS)) * i128::from(ANGLE_STEPS);
    let step = (scaled / i128::from(TURN_UNITS)) as u32;
    let fraction = scaled % i128::from(TURN_UNITS);
    let from = rotate_vector(step, point.0, point.1);
    let to = rotate_vector((step + 1) % ANGLE_STEPS, point.0, point.1);
    (
        from.0 + round_div(i128::from(to.0 - from.0) * fraction, i128::from(TURN_UNITS)) as i64,
        from.1 + round_div(i128::from(to.1 - from.1) * fraction, i128::from(TURN_UNITS)) as i64,
    )
}

fn pivot_motion(angular: i64, point: (i64, i64)) -> Motion {
    Motion {
        x: round_div(
            TAU_NUMERATOR * i128::from(angular) * i128::from(point.1),
            ANGULAR_DENOMINATOR,
        ) as i64,
        y: round_div(
            -TAU_NUMERATOR * i128::from(angular) * i128::from(point.0),
            ANGULAR_DENOMINATOR,
        ) as i64,
        angular,
    }
}

fn constraint_velocity(motion: Motion, constraint: Constraint) -> i128 {
    ANGULAR_DENOMINATOR
        * (i128::from(motion.x) * i128::from(constraint.nx)
            + i128::from(motion.y) * i128::from(constraint.ny))
        + TAU_NUMERATOR * i128::from(motion.angular) * constraint.lever
}

fn project_plane(body: &PixelBody, motion: PreciseMotion, row: ConstraintRow) -> PreciseMotion {
    let mass = i128::from(body.mass);
    let inertia = body.inertia.max(1);
    let dot = row.x * motion.x + row.y * motion.y + row.angular * motion.angular;
    let denominator = inertia * (row.x * row.x + row.y * row.y) + mass * row.angular * row.angular;
    if denominator == 0 {
        return motion;
    }
    PreciseMotion {
        x: motion.x - round_div(inertia * row.x * dot, denominator),
        y: motion.y - round_div(inertia * row.y * dot, denominator),
        angular: motion.angular - round_div(mass * row.angular * dot, denominator),
    }
}

fn nearest_motion(motion: PreciseMotion) -> Motion {
    Motion {
        x: clamp_i128(round_div(motion.x, ANGULAR_DENOMINATOR * SOLVER_SCALE)),
        y: clamp_i128(round_div(motion.y, ANGULAR_DENOMINATOR * SOLVER_SCALE)),
        angular: clamp_i128(round_div(motion.angular, TAU_NUMERATOR * SOLVER_SCALE)),
    }
}

fn motion_distance(body: &PixelBody, left: Motion, right: Motion) -> i128 {
    let dx = ANGULAR_DENOMINATOR * (i128::from(left.x) - i128::from(right.x));
    let dy = ANGULAR_DENOMINATOR * (i128::from(left.y) - i128::from(right.y));
    let angular = TAU_NUMERATOR * (i128::from(left.angular) - i128::from(right.angular));
    i128::from(body.mass) * (dx * dx + dy * dy) + body.inertia * angular * angular
}

fn kinetic_energy(body: &PixelBody, motion: Motion) -> i128 {
    let x = ANGULAR_DENOMINATOR * i128::from(motion.x);
    let y = ANGULAR_DENOMINATOR * i128::from(motion.y);
    let angular = TAU_NUMERATOR * i128::from(motion.angular);
    i128::from(body.mass) * (x * x + y * y) + body.inertia * angular * angular
}

fn rasterize(raster: &mut Raster, cells: &[BodyCell], x: Subcell, y: Subcell, angle: i64) {
    raster.clear();
    let step = quantize_step(angle);
    let sum = cells.iter().fold((0i64, 0i64), |sum, cell| {
        let (dx, dy) = rotate_offset(step, cell.local.0, cell.local.1);
        (sum.0 + i64::from(dx), sum.1 + i64::from(dy))
    });
    let count = cells.len() as i128;
    let mean_x = round_div(
        i128::from(sum.0) * i128::from(SUBCELL_UNITS_PER_CELL),
        count,
    ) as i64;
    let mean_y = round_div(
        i128::from(sum.1) * i128::from(SUBCELL_UNITS_PER_CELL),
        count,
    ) as i64;
    let pivot = CellPos::new(
        Subcell::from_raw(x.raw() - mean_x).floor_cell(),
        Subcell::from_raw(y.raw() - mean_y).floor_cell(),
    );
    for (index, cell) in cells.iter().enumerate() {
        let (dx, dy) = rotate_offset(step, cell.local.0, cell.local.1);
        let pos = pivot.translated(dx, dy);
        let inserted = raster.set.insert(pos);
        debug_assert!(inserted, "body rotation must be bijective");
        raster.cells.push((pos, index as u16));
    }
}

fn commit(world: &mut CellWorld, body: &PixelBody, new: &Raster) {
    let mut displaced: Vec<_> = new
        .cells
        .iter()
        .filter(|(pos, _)| !body.raster.covers(*pos))
        .map(|(pos, _)| world.get_cell(*pos).expect("body proposal is loaded"))
        .collect();
    displaced.sort_unstable_by_key(|cell| std::cmp::Reverse(content::density_milli(cell.material)));
    let mut vacated: Vec<_> = body
        .raster
        .cells
        .iter()
        .filter_map(|&(pos, _)| (!new.covers(pos)).then_some(pos))
        .collect();
    vacated.sort_unstable_by_key(|pos| (pos.y, pos.x));
    debug_assert_eq!(vacated.len(), displaced.len());
    for (pos, cell) in vacated.into_iter().zip(displaced) {
        world.set_cell_raw(pos, cell);
    }
    for &(pos, local) in &new.cells {
        let cell = body.cells[local as usize].cell;
        if world.get_cell(pos) != Some(cell) {
            world.set_cell_raw(pos, cell);
        }
    }
}

fn settle(world: &mut CellWorld, body: &PixelBody) {
    for &(pos, local) in &body.raster.cells {
        let mut cell = body.cells[local as usize].cell;
        cell.set_body(false);
        world.set_cell_raw(pos, cell);
    }
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

fn split_step(remainder: &mut i128, motion: i64, substeps: u32) -> i64 {
    *remainder += i128::from(motion);
    let step = *remainder / i128::from(substeps);
    *remainder %= i128::from(substeps);
    step as i64
}

fn clamp_i128(value: i128) -> i64 {
    value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}
