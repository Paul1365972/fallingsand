use super::contact::{CellState, Contact, Peer, Resolver};
use super::rotation::{ANGLE_STEPS, ORIENTATION_UNITS, Spin};
use super::state::{Body, CELL, Freedoms, cell_mass, rasterize, rotated_mean};
use crate::motion::{GRAVITY_DV, MAX_SPEED_CELLS, SETTLE};
use crate::world::CellWorld;
use fallingsand_core::{
    CARDINAL_NEIGHBORS, Cell, CellPos, ChunkPos, MaterialId, Phase, Q16, content,
};
use fallingsand_math::round_div;
use rustc_hash::{FxHashMap, FxHashSet};

const GRAVITY: i64 = GRAVITY_DV as i64;
const MAX_BODY_SPEED_CELLS: i64 = MAX_SPEED_CELLS as i64;
const MAX_SPEED: i64 = MAX_BODY_SPEED_CELLS * CELL;
const MAX_TURN_QUANTA: i64 = ANGLE_STEPS as i64;
const FLUID_DRAG_DIVISOR: i64 = 4;
const SNAP: i64 = 16;
const SETTLE_EPSILON: i64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Freedom {
    Turn,
    X,
    Y,
}

impl Freedom {
    const ALL: [Self; 3] = [Self::Y, Self::X, Self::Turn];

    fn index(self) -> usize {
        match self {
            Self::Turn => 0,
            Self::X => 1,
            Self::Y => 2,
        }
    }

    fn bit(self) -> u8 {
        1 << self.index()
    }

    fn threshold(self) -> i64 {
        match self {
            Self::Turn => ORIENTATION_UNITS,
            Self::X | Self::Y => CELL,
        }
    }

    fn accumulator(self, body: &Body) -> i64 {
        match self {
            Self::Turn => body.acc_turn,
            Self::X => body.acc_x,
            Self::Y => body.acc_y,
        }
    }

    fn accumulator_mut(self, body: &mut Body) -> &mut i64 {
        match self {
            Self::Turn => &mut body.acc_turn,
            Self::X => &mut body.acc_x,
            Self::Y => &mut body.acc_y,
        }
    }

    fn velocity(self, body: &Body) -> i64 {
        match self {
            Self::Turn => body.spin.raw(),
            Self::X => body.vx,
            Self::Y => body.vy,
        }
    }

    fn pending(self, body: &Body) -> i64 {
        self.accumulator(body).abs() / self.threshold()
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct FreedomState {
    parked: bool,
    probed: bool,
}

#[derive(Debug, Default, Clone, Copy)]
struct Mover {
    freedoms: [FreedomState; 3],
}

struct Proposal {
    body: usize,
    freedom: Freedom,
    sign: i32,
    translation: (i32, i32),
    probe: bool,
    new_step: u32,
    candidate: Vec<CellPos>,
    entered: Vec<(CellPos, CellPos)>,
    refused: bool,
    committed: bool,
    canceled: bool,
    deps: Vec<(usize, CellPos, CellPos)>,
    contacts: Vec<Contact>,
}

pub(super) fn integrate_forces(world: &mut CellWorld, body: &mut Body) {
    let com = body.com();
    let displaced = displaced_medium(world, body);
    let mut gravity = 0i128;
    let mut gravity_torque = 0i128;
    let mut fluid = FxHashSet::default();
    for (slot, &pos) in body.slots.iter().zip(&body.raster) {
        for (dx, dy) in CARDINAL_NEIGHBORS {
            let near = pos.translated(dx, dy);
            let Some(cell) = world.get_cell(near) else {
                continue;
            };
            if cell.body_id().is_none() && content::phase(cell.material) == Phase::Liquid {
                fluid.insert(near);
            }
        }
        let weight = cell_weight(slot.material, &displaced, pos);
        gravity += weight;
        gravity_torque += i128::from(super::state::cell_center(pos.x) - com.0) * weight;
    }
    let waterline = body.settles && at_waterline(body, &displaced, gravity);
    if waterline {
        gravity = 0;
    }
    body.weight = (gravity / i128::from(body.mass)) as i64;
    body.vy += body.weight;
    if body.freedoms.holds(Freedoms::TURN) {
        body.spin += Spin::from_angular_impulse(gravity_torque, body.moment);
    }
    apply_fluid_drag(world, body, fluid);
    if waterline {
        if body.vx.abs() < i64::from(SETTLE) {
            body.vx = 0;
        }
        if body.vy.abs() < i64::from(SETTLE) {
            body.vy = 0;
        }
        let spin_floor = Spin::for_speed_at(i64::from(SETTLE), body.radius.max(1) * CELL);
        if body.spin.clamped(spin_floor) == body.spin {
            body.spin = Spin::ZERO;
        }
    }
    body.vx = body.vx.clamp(-MAX_SPEED, MAX_SPEED);
    body.vy = body.vy.clamp(-MAX_SPEED, MAX_SPEED);
    let turn_cap = Spin::for_speed_at(MAX_SPEED, body.radius.max(1) * CELL)
        .clamped(Spin::from_raw(MAX_TURN_QUANTA * ORIENTATION_UNITS));
    body.spin = body.spin.clamped(turn_cap);
}

fn apply_fluid_drag(world: &mut CellWorld, body: &mut Body, positions: FxHashSet<CellPos>) {
    let cells: Vec<_> = positions
        .into_iter()
        .filter_map(|pos| {
            let cell = world.get_cell(pos)?;
            let mass = i64::from(content::density_milli(cell.material).max(1));
            Some((pos, cell, mass))
        })
        .collect();
    let fluid_mass: i64 = cells.iter().map(|(_, _, mass)| mass).sum();
    if fluid_mass == 0 {
        return;
    }
    let momentum = cells.iter().fold((0i128, 0i128), |sum, (_, cell, mass)| {
        let (vx, vy) = cell.vel();
        (
            sum.0 + i128::from(vx) * i128::from(*mass),
            sum.1 + i128::from(vy) * i128::from(*mass),
        )
    });
    let mean = (
        (momentum.0 / i128::from(fluid_mass)) as i64,
        (momentum.1 / i128::from(fluid_mass)) as i64,
    );
    let reduced =
        i128::from(body.mass) * i128::from(fluid_mass) / i128::from(body.mass + fluid_mass);
    let impulse = (
        i128::from(mean.0 - body.vx) * reduced / i128::from(FLUID_DRAG_DIVISOR),
        i128::from(mean.1 - body.vy) * reduced / i128::from(FLUID_DRAG_DIVISOR),
    );
    if impulse == (0, 0) {
        return;
    }
    let fluid_delta = (
        (-impulse.0 / i128::from(fluid_mass)) as i64,
        (-impulse.1 / i128::from(fluid_mass)) as i64,
    );
    let next_body = (
        body.vx + (impulse.0 / i128::from(body.mass)) as i64,
        body.vy + (impulse.1 / i128::from(body.mass)) as i64,
    );
    let kinetic = |mass: i64, vx: i64, vy: i64| {
        i128::from(mass) * (i128::from(vx) * i128::from(vx) + i128::from(vy) * i128::from(vy))
    };
    let before = cells.iter().fold(
        kinetic(body.mass, body.vx, body.vy),
        |energy, (_, cell, mass)| {
            let (vx, vy) = cell.vel();
            energy + kinetic(*mass, i64::from(vx), i64::from(vy))
        },
    );
    let after = cells.iter().fold(
        kinetic(body.mass, next_body.0, next_body.1),
        |energy, (_, cell, mass)| {
            let (vx, vy) = cell.vel();
            energy
                + kinetic(
                    *mass,
                    i64::from(vx) + fluid_delta.0,
                    i64::from(vy) + fluid_delta.1,
                )
        },
    );
    if after > before {
        return;
    }
    (body.vx, body.vy) = next_body;
    if fluid_delta == (0, 0) {
        return;
    }
    for (pos, mut cell, _) in cells {
        let (vx, vy) = cell.vel();
        cell.set_vel(
            (i64::from(vx) + fluid_delta.0) as i32,
            (i64::from(vy) + fluid_delta.1) as i32,
        );
        world.set(pos, cell);
    }
}

pub(super) struct Displaced {
    rows: FxHashMap<i32, (i128, i128)>,
    mean: i64,
}

impl Displaced {
    fn at(&self, y: i32) -> i64 {
        match self.rows.get(&y) {
            Some(&(total, faces)) => round_div(total, faces) as i64,
            None => self.mean,
        }
    }
}

pub(super) fn displaced_medium(world: &CellWorld, body: &Body) -> Displaced {
    let mut rows: FxHashMap<i32, (i128, i128)> = FxHashMap::default();
    let mut total = 0i128;
    let mut faces = 0i128;
    for &pos in &body.raster {
        for (dx, dy) in CARDINAL_NEIGHBORS {
            let near = pos.translated(dx, dy);
            let Some(cell) = world.get_cell(near) else {
                continue;
            };
            if cell.body_id().is_some() {
                continue;
            }
            if !matches!(
                content::phase(cell.material),
                Phase::Liquid | Phase::Gas | Phase::Empty
            ) {
                continue;
            }
            let density = i128::from(content::density_milli(cell.material).max(1));
            let row = rows.entry(near.y).or_insert((0, 0));
            row.0 += density;
            row.1 += 1;
            total += density;
            faces += 1;
        }
    }
    let mean = if faces == 0 {
        0
    } else {
        round_div(total, faces) as i64
    };
    Displaced { rows, mean }
}

fn cell_weight(material: MaterialId, displaced: &Displaced, pos: CellPos) -> i128 {
    i128::from(-GRAVITY) * i128::from(cell_mass(material) - displaced.at(pos.y))
}

fn at_waterline(body: &Body, displaced: &Displaced, lift: i128) -> bool {
    lift > 0
        && body
            .slots
            .iter()
            .zip(&body.raster)
            .map(|(slot, &pos)| cell_weight(slot.material, displaced, pos.translated(0, 1)))
            .sum::<i128>()
            <= 0
}

pub(super) fn net_lift(world: &CellWorld, body: &Body) -> i128 {
    let displaced = displaced_medium(world, body);
    body.slots
        .iter()
        .zip(&body.raster)
        .map(|(slot, &pos)| cell_weight(slot.material, &displaced, pos))
        .sum()
}

pub(super) fn run_rounds<S>(
    world: &mut CellWorld,
    bodies: &mut [Body],
    by_id: &FxHashMap<u32, usize>,
    simulated: &S,
    cells: &mut FxHashMap<CellPos, CellState>,
) where
    S: Fn(ChunkPos) -> bool,
{
    let mut movers = vec![Mover::default(); bodies.len()];
    for body in bodies.iter_mut() {
        if body.parked {
            continue;
        }
        let floor = if body.settles { i64::from(SETTLE) } else { 1 };
        body.acc_x = if body.vx.abs() >= floor {
            (body.acc_x + body.vx).clamp(-MAX_SPEED, MAX_SPEED)
        } else {
            body.acc_x / 2
        };
        body.acc_y = if body.vy.abs() >= floor {
            (body.acc_y + body.vy).clamp(-MAX_SPEED, MAX_SPEED)
        } else {
            body.acc_y / 2
        };
        let spin_floor = Spin::for_speed_at(floor, body.radius.max(1) * CELL);
        body.acc_turn = if body.spin.clamped(spin_floor) != body.spin {
            (body.acc_turn + body.spin.raw()).clamp(
                -MAX_TURN_QUANTA * ORIENTATION_UNITS,
                MAX_TURN_QUANTA * ORIENTATION_UNITS,
            )
        } else {
            body.acc_turn / 2
        };
    }

    loop {
        let mut proposals = collect_proposals(world, bodies, &mut movers);
        if proposals.is_empty() {
            break;
        }
        let mut newly_parked = Vec::new();
        classify(
            world,
            bodies,
            by_id,
            simulated,
            cells,
            &mut movers,
            &mut proposals,
            &mut newly_parked,
        );
        apply_assists(world, bodies, simulated, &mut proposals);
        for index in newly_parked {
            bodies[index].parked = true;
        }
        resolve_ties(bodies, &mut proposals);
        cascade(bodies, &mut proposals);
        commit(world, bodies, &mut proposals, by_id, cells);
        finish_round(world, bodies, &mut movers, &mut proposals, cells);
    }
}

fn collect_proposals(world: &CellWorld, bodies: &[Body], movers: &mut [Mover]) -> Vec<Proposal> {
    let mut proposals = Vec::new();
    for (index, body) in bodies.iter().enumerate() {
        if body.parked {
            continue;
        }
        let mover = &movers[index];
        let mut best: Option<Freedom> = None;
        for freedom in Freedom::ALL {
            if !body.freedoms.holds(freedom.bit())
                || mover.freedoms[freedom.index()].parked
                || freedom.pending(body) == 0
            {
                continue;
            }
            best = match best {
                Some(current) if freedom.pending(body) <= current.pending(body) => Some(current),
                _ => Some(freedom),
            };
        }
        if let Some(freedom) = best {
            let sign = freedom.accumulator(body).signum() as i32;
            proposals.push(build_proposal(world, bodies, index, freedom, sign, false));
            continue;
        }
        for freedom in Freedom::ALL {
            let state = mover.freedoms[freedom.index()];
            if !body.freedoms.holds(freedom.bit())
                || state.parked
                || state.probed
                || freedom.velocity(body) == 0
            {
                continue;
            }
            let sign = freedom.velocity(body).signum() as i32;
            proposals.push(build_proposal(world, bodies, index, freedom, sign, true));
        }
    }
    proposals
}

fn build_proposal(
    world: &CellWorld,
    bodies: &[Body],
    index: usize,
    freedom: Freedom,
    sign: i32,
    probe: bool,
) -> Proposal {
    let body = &bodies[index];
    let mut candidate = Vec::new();
    let mut entered = Vec::new();
    let mut new_step = body.step;
    let mut translation = (0, 0);
    let not_mine = |pos: CellPos| {
        world
            .get_cell(pos)
            .is_none_or(|cell| cell.body_id() != Some(body.id))
    };
    match freedom {
        Freedom::X | Freedom::Y => {
            let (dx, dy) = if freedom == Freedom::X {
                (sign, 0)
            } else {
                (0, sign)
            };
            translation = (dx, dy);
            candidate.extend(body.raster.iter().map(|pos| pos.translated(dx, dy)));
            for &pos in &candidate {
                if not_mine(pos) {
                    entered.push((pos, pos.translated(-dx, -dy)));
                }
            }
        }
        Freedom::Turn => {
            new_step = (body.step as i32 + sign).rem_euclid(ANGLE_STEPS as i32) as u32;
            rasterize(&body.slots, body.anchor, new_step, &mut candidate);
            let mut seen = FxHashSet::default();
            for (slot, &to) in body.raster.iter().zip(&candidate) {
                walk_line(*slot, to, |at, from| {
                    if not_mine(at) && seen.insert(at) {
                        entered.push((at, from));
                    }
                });
            }
        }
    }
    Proposal {
        body: index,
        freedom,
        sign,
        translation,
        probe,
        new_step,
        candidate,
        entered,
        refused: false,
        committed: false,
        canceled: false,
        deps: Vec::new(),
        contacts: Vec::new(),
    }
}

fn build_step_proposal(
    world: &CellWorld,
    bodies: &[Body],
    index: usize,
    sign: i32,
    rise: i32,
) -> Proposal {
    let body = &bodies[index];
    let mut candidate = Vec::with_capacity(body.raster.len());
    let mut entered = Vec::new();
    let mut seen = FxHashSet::default();
    let not_mine = |pos: CellPos| {
        world
            .get_cell(pos)
            .is_none_or(|cell| cell.body_id() != Some(body.id))
    };
    candidate.extend(body.raster.iter().map(|pos| pos.translated(sign, rise)));
    for &pos in &body.raster {
        let mut from = pos;
        for dy in 1..=rise {
            let at = pos.translated(0, dy);
            if not_mine(at) && seen.insert(at) {
                entered.push((at, from));
            }
            from = at;
        }
        let at = pos.translated(sign, rise);
        if not_mine(at) && seen.insert(at) {
            entered.push((at, from));
        }
    }
    Proposal {
        body: index,
        freedom: Freedom::X,
        sign,
        translation: (sign, rise),
        probe: false,
        new_step: body.step,
        candidate,
        entered,
        refused: false,
        committed: false,
        canceled: false,
        deps: Vec::new(),
        contacts: Vec::new(),
    }
}

fn walk_line(from: CellPos, to: CellPos, mut visit: impl FnMut(CellPos, CellPos)) {
    let (mut x, mut y) = (from.x, from.y);
    let (adx, ady) = ((to.x - from.x).abs(), (to.y - from.y).abs());
    let (sx, sy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
    let mut err = adx - ady;
    let mut prev = from;
    while (x, y) != (to.x, to.y) {
        let step_x = x != to.x && (2 * err > -ady || y == to.y);
        if step_x {
            err -= ady;
            x += sx;
        } else {
            err += adx;
            y += sy;
        }
        let cur = CellPos::new(x, y);
        visit(cur, prev);
        prev = cur;
    }
}

#[allow(clippy::too_many_arguments)]
fn classify<S>(
    world: &CellWorld,
    bodies: &[Body],
    by_id: &FxHashMap<u32, usize>,
    simulated: &S,
    cells: &mut FxHashMap<CellPos, CellState>,
    movers: &mut [Mover],
    proposals: &mut [Proposal],
    newly_parked: &mut Vec<usize>,
) where
    S: Fn(ChunkPos) -> bool,
{
    let mut proposal_of_body: FxHashMap<usize, usize> = FxHashMap::default();
    for (p, proposal) in proposals.iter().enumerate() {
        if !proposal.probe {
            proposal_of_body.insert(proposal.body, p);
        }
    }

    for p in 0..proposals.len() {
        let body_index = proposals[p].body;
        let my = &bodies[body_index];
        let mut busy = false;
        let mut unloaded = false;
        let entered = std::mem::take(&mut proposals[p].entered);
        for &(at, from) in &entered {
            let Some(cell) = world.get_cell(at) else {
                unloaded = true;
                continue;
            };
            if !simulated(at.chunk()) {
                unloaded = true;
                continue;
            }
            match cell.body_id() {
                Some(id) if id == my.id => {}
                Some(id) => {
                    if let Some(&peer_index) = by_id.get(&id) {
                        let peer = &bodies[peer_index];
                        let crossing = i128::from(my.vx) * i128::from(peer.vx)
                            + i128::from(my.vy) * i128::from(peer.vy)
                            < 0;
                        let moving = proposal_of_body.contains_key(&peer_index);
                        if moving && !crossing && !proposals[p].probe {
                            proposals[p]
                                .deps
                                .push((proposal_of_body[&peer_index], at, from));
                        } else {
                            push_contact(
                                &mut proposals[p],
                                body_index,
                                at,
                                from,
                                Peer::Body(peer_index),
                                pair_restitution(my, Some(peer), Some(cell.material)),
                                my.friction.mul(peer.friction),
                            );
                        }
                    } else {
                        push_contact(
                            &mut proposals[p],
                            body_index,
                            at,
                            from,
                            Peer::Terrain,
                            pair_restitution(my, None, Some(cell.material)),
                            my.friction.mul(content::friction(cell.material)),
                        );
                    }
                }
                None => match content::phase(cell.material) {
                    Phase::Solid => {
                        push_contact(
                            &mut proposals[p],
                            body_index,
                            at,
                            from,
                            Peer::Terrain,
                            pair_restitution(my, None, Some(cell.material)),
                            my.friction.mul(content::friction(cell.material)),
                        );
                    }
                    Phase::Powder => {
                        let peer = powder_peer(my, cell, at, from, cells);
                        push_contact(
                            &mut proposals[p],
                            body_index,
                            at,
                            from,
                            peer,
                            pair_restitution(my, None, Some(cell.material)),
                            my.friction.mul(content::friction(cell.material)),
                        );
                    }
                    _ => {
                        if !cell.is_air() && cell.is_moved() {
                            busy = true;
                        }
                    }
                },
            }
        }
        proposals[p].entered = entered;
        if unloaded {
            bodies_parked(movers, proposals, body_index);
            newly_parked.push(body_index);
            proposals[p].canceled = true;
            continue;
        }
        if !proposals[p].contacts.is_empty() {
            proposals[p].refused = true;
        } else if busy {
            movers[body_index].freedoms[proposals[p].freedom.index()].parked = true;
            proposals[p].canceled = true;
        }
    }
}

fn apply_assists<S>(world: &CellWorld, bodies: &[Body], simulated: &S, proposals: &mut [Proposal])
where
    S: Fn(ChunkPos) -> bool,
{
    for proposal in proposals {
        if proposal.probe
            || proposal.freedom != Freedom::X
            || !proposal.refused
            || !bodies[proposal.body].assists
            || !body_grounded(world, &bodies[proposal.body])
        {
            continue;
        }
        let replacement = (1..=STEP_CELLS)
            .map(|rise| build_step_proposal(world, bodies, proposal.body, proposal.sign, rise))
            .find(|candidate| assist_path_clear(world, simulated, candidate));
        if let Some(replacement) = replacement {
            *proposal = replacement;
        }
    }
}

fn assist_path_clear<S>(world: &CellWorld, simulated: &S, proposal: &Proposal) -> bool
where
    S: Fn(ChunkPos) -> bool,
{
    proposal.entered.iter().all(|&(at, _)| {
        simulated(at.chunk())
            && world.get_cell(at).is_some_and(|cell| {
                cell.body_id().is_none()
                    && matches!(
                        content::phase(cell.material),
                        Phase::Empty | Phase::Liquid | Phase::Gas
                    )
                    && (cell.is_air() || !cell.is_moved())
            })
    })
}

fn bodies_parked(movers: &mut [Mover], proposals: &mut [Proposal], body: usize) {
    for state in &mut movers[body].freedoms {
        state.parked = true;
    }
    for proposal in proposals.iter_mut() {
        if proposal.body == body {
            proposal.canceled = true;
        }
    }
}

fn pair_restitution(body: &Body, peer: Option<&Body>, material: Option<MaterialId>) -> Q16 {
    let mut restitution = body.restitution;
    if let Some(material) = material {
        let material_restitution = content::restitution(material);
        if material_restitution.raw() > restitution.raw() {
            restitution = material_restitution;
        }
    }
    if let Some(peer) = peer
        && peer.restitution.raw() > restitution.raw()
    {
        restitution = peer.restitution;
    }
    restitution
}

fn powder_peer(
    body: &Body,
    cell: Cell,
    at: CellPos,
    from: CellPos,
    cells: &mut FxHashMap<CellPos, CellState>,
) -> Peer {
    let mass = cell_mass(cell.material);
    let com = body.com();
    let point = body.point_velocity(com, at);
    let normal = (from.x - at.x, from.y - at.y);
    let closing =
        i128::from(point.0) * i128::from(-normal.0) + i128::from(point.1) * i128::from(-normal.1);
    let resistance =
        i128::from(content::repose_layers(cell.material)) * i128::from(GRAVITY) * i128::from(mass);
    let effective = i128::from(body.mass) * i128::from(mass) / i128::from(body.mass + mass);
    let needed = closing.max(0) * effective;
    if needed / i128::from(normal.0 * normal.0 + normal.1 * normal.1).max(1) <= resistance {
        return Peer::Terrain;
    }
    let (cvx, cvy) = cell.vel();
    let vy = if cell.is_stressed() {
        0
    } else {
        i64::from(cvy)
    };
    cells.entry(at).or_insert(CellState {
        mass,
        vx: i64::from(cvx),
        vy,
        start_vx: i64::from(cvx),
        start_vy: vy,
    });
    Peer::Cell { pos: at, mass }
}

fn push_contact(
    proposal: &mut Proposal,
    body: usize,
    at: CellPos,
    from: CellPos,
    peer: Peer,
    restitution: Q16,
    friction: Q16,
) {
    let normal = (from.x - at.x, from.y - at.y);
    proposal.contacts.push(Contact {
        body,
        from,
        at,
        normal,
        peer,
        restitution,
        friction,
        target: 0,
        push: 0,
        drag: 0,
    });
}

fn resolve_ties(bodies: &[Body], proposals: &mut [Proposal]) {
    let mut claims: FxHashMap<CellPos, Vec<usize>> = FxHashMap::default();
    for (p, proposal) in proposals.iter().enumerate() {
        if proposal.probe || proposal.canceled || proposal.refused {
            continue;
        }
        for &(at, _) in &proposal.entered {
            claims.entry(at).or_default().push(p);
        }
    }
    let mut losses: Vec<(usize, CellPos, CellPos, usize)> = Vec::new();
    for (at, claimants) in claims {
        if claimants.len() < 2 {
            continue;
        }
        let momentum = |p: usize| {
            let proposal = &proposals[p];
            let body = &bodies[proposal.body];
            let from = proposal
                .entered
                .iter()
                .find(|&&(cell, _)| cell == at)
                .map(|&(_, from)| from)
                .expect("claimant entered the cell");
            let dir = (at.x - from.x, at.y - from.y);
            let com = body.com();
            let point = body.point_velocity(com, at);
            let closing =
                i128::from(point.0) * i128::from(dir.0) + i128::from(point.1) * i128::from(dir.1);
            closing.max(0) * i128::from(body.mass)
        };
        let winner = claimants
            .iter()
            .copied()
            .max_by_key(|&p| (momentum(p), std::cmp::Reverse(bodies[proposals[p].body].id)))
            .expect("claimants are non-empty");
        for p in claimants {
            if p == winner {
                continue;
            }
            let from = proposals[p]
                .entered
                .iter()
                .find(|&&(cell, _)| cell == at)
                .map(|&(_, from)| from)
                .expect("claimant entered the cell");
            losses.push((p, at, from, proposals[winner].body));
        }
    }
    for (p, at, from, winner_body) in losses {
        let body_index = proposals[p].body;
        let my = &bodies[body_index];
        let peer = &bodies[winner_body];
        push_contact(
            &mut proposals[p],
            body_index,
            at,
            from,
            Peer::Body(winner_body),
            pair_restitution(my, Some(peer), None),
            my.friction.mul(peer.friction),
        );
        proposals[p].refused = true;
    }
}

fn cascade(bodies: &[Body], proposals: &mut [Proposal]) {
    loop {
        let mut changed = false;
        for p in 0..proposals.len() {
            if proposals[p].refused || proposals[p].canceled || proposals[p].probe {
                continue;
            }
            let deps = std::mem::take(&mut proposals[p].deps);
            let mut blocked = Vec::new();
            for &(dep, at, from) in &deps {
                if proposals[dep].refused || proposals[dep].canceled {
                    blocked.push((at, from, proposals[dep].body));
                }
            }
            if blocked.is_empty() {
                proposals[p].deps = deps;
                continue;
            }
            let body_index = proposals[p].body;
            for (at, from, peer_body) in blocked {
                let my = &bodies[body_index];
                let peer = &bodies[peer_body];
                push_contact(
                    &mut proposals[p],
                    body_index,
                    at,
                    from,
                    Peer::Body(peer_body),
                    pair_restitution(my, Some(peer), None),
                    my.friction.mul(peer.friction),
                );
            }
            proposals[p].refused = true;
            changed = true;
        }
        if !changed {
            break;
        }
    }
}

fn commit(
    world: &mut CellWorld,
    bodies: &mut [Body],
    proposals: &mut [Proposal],
    by_id: &FxHashMap<u32, usize>,
    cells: &mut FxHashMap<CellPos, CellState>,
) {
    loop {
        let mut progress = false;
        for p in 0..proposals.len() {
            if proposals[p].refused
                || proposals[p].canceled
                || proposals[p].committed
                || proposals[p].probe
            {
                continue;
            }
            let ready = proposals[p]
                .deps
                .iter()
                .all(|&(dep, _, _)| proposals[dep].committed);
            if !ready {
                continue;
            }
            if revalidate(world, bodies, &proposals[p]) {
                commit_group(world, bodies, &[p], proposals);
                proposals[p].committed = true;
            } else {
                refuse_after_commit(world, bodies, by_id, cells, proposals, p);
            }
            progress = true;
        }
        if !progress {
            break;
        }
    }

    let mut remaining: Vec<usize> = (0..proposals.len())
        .filter(|&p| {
            !proposals[p].refused
                && !proposals[p].canceled
                && !proposals[p].committed
                && !proposals[p].probe
        })
        .collect();
    while let Some(&start) = remaining.first() {
        let mut group = vec![start];
        let mut frontier = vec![start];
        while let Some(p) = frontier.pop() {
            for &(dep, _, _) in &proposals[p].deps {
                if remaining.contains(&dep) && !group.contains(&dep) {
                    group.push(dep);
                    frontier.push(dep);
                }
            }
        }
        group.sort_unstable();
        remaining.retain(|p| !group.contains(p));
        if joint_valid(world, bodies, proposals, &group) {
            commit_group(world, bodies, &group, proposals);
            for &p in &group {
                proposals[p].committed = true;
            }
        } else {
            for &p in &group {
                refuse_after_commit(world, bodies, by_id, cells, proposals, p);
            }
        }
    }
}

fn revalidate(world: &CellWorld, bodies: &[Body], proposal: &Proposal) -> bool {
    let my = &bodies[proposal.body];
    proposal.entered.iter().all(|&(at, _)| {
        world.get_cell(at).is_some_and(|cell| {
            cell.body_id() == Some(my.id)
                || (cell.body_id().is_none()
                    && matches!(
                        content::phase(cell.material),
                        Phase::Empty | Phase::Liquid | Phase::Gas
                    ))
        })
    })
}

fn joint_valid(
    world: &CellWorld,
    bodies: &[Body],
    proposals: &[Proposal],
    group: &[usize],
) -> bool {
    let mut claimed = FxHashSet::default();
    for &p in group {
        for &pos in &proposals[p].candidate {
            if !claimed.insert(pos) {
                return false;
            }
        }
    }
    let members: FxHashMap<u32, usize> = group
        .iter()
        .map(|&p| (bodies[proposals[p].body].id, p))
        .collect();
    for &p in group {
        for &(at, _) in &proposals[p].entered {
            let Some(cell) = world.get_cell(at) else {
                return false;
            };
            match cell.body_id() {
                None => {
                    if !matches!(
                        content::phase(cell.material),
                        Phase::Empty | Phase::Liquid | Phase::Gas
                    ) {
                        return false;
                    }
                }
                Some(id) => {
                    let Some(&owner) = members.get(&id) else {
                        return false;
                    };
                    if proposals[owner].candidate.contains(&at) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn refuse_after_commit(
    world: &CellWorld,
    bodies: &[Body],
    by_id: &FxHashMap<u32, usize>,
    cells: &mut FxHashMap<CellPos, CellState>,
    proposals: &mut [Proposal],
    p: usize,
) {
    let body_index = proposals[p].body;
    let entered = proposals[p].entered.clone();
    for (at, from) in entered {
        let Some(cell) = world.get_cell(at) else {
            continue;
        };
        let blocked = match cell.body_id() {
            Some(id) if id == bodies[body_index].id => false,
            Some(_) => true,
            None => matches!(content::phase(cell.material), Phase::Solid | Phase::Powder),
        };
        if !blocked {
            continue;
        }
        let my = &bodies[body_index];
        match cell.body_id() {
            Some(id) => {
                if let Some(&peer_index) = by_id.get(&id) {
                    let peer = &bodies[peer_index];
                    push_contact(
                        &mut proposals[p],
                        body_index,
                        at,
                        from,
                        Peer::Body(peer_index),
                        pair_restitution(my, Some(peer), Some(cell.material)),
                        my.friction.mul(peer.friction),
                    );
                } else {
                    push_contact(
                        &mut proposals[p],
                        body_index,
                        at,
                        from,
                        Peer::Terrain,
                        pair_restitution(my, None, Some(cell.material)),
                        my.friction.mul(content::friction(cell.material)),
                    );
                }
            }
            None => {
                let peer = if content::phase(cell.material) == Phase::Powder {
                    powder_peer(my, cell, at, from, cells)
                } else {
                    Peer::Terrain
                };
                push_contact(
                    &mut proposals[p],
                    body_index,
                    at,
                    from,
                    peer,
                    pair_restitution(my, None, Some(cell.material)),
                    my.friction.mul(content::friction(cell.material)),
                );
            }
        }
    }
    proposals[p].refused = true;
}

fn commit_group(
    world: &mut CellWorld,
    bodies: &mut [Body],
    group: &[usize],
    proposals: &mut [Proposal],
) {
    let mut current: FxHashSet<CellPos> = FxHashSet::default();
    let mut next: FxHashSet<CellPos> = FxHashSet::default();
    for &p in group {
        current.extend(bodies[proposals[p].body].raster.iter().copied());
        next.extend(proposals[p].candidate.iter().copied());
    }
    let mut vacated: Vec<CellPos> = current.difference(&next).copied().collect();
    vacated.sort_unstable_by_key(|pos| (pos.y, pos.x));
    let mut claimed: Vec<CellPos> = next.difference(&current).copied().collect();
    claimed.sort_unstable_by_key(|pos| (pos.y, pos.x));

    let mut moving: Vec<(usize, Vec<Cell>)> = group
        .iter()
        .map(|&p| {
            let body = &bodies[proposals[p].body];
            (
                p,
                body.raster
                    .iter()
                    .map(|&pos| world.get_cell(pos).expect("body raster is loaded"))
                    .collect(),
            )
        })
        .collect();

    let mut displaced: Vec<Cell> = Vec::with_capacity(claimed.len());
    for &pos in &claimed {
        let mut cell = world.get_cell(pos).expect("claimed cell is loaded");
        if !cell.is_air() {
            let (owner, entrained) = entraining_body(bodies, proposals, group, pos);
            let mass = i128::from(cell_mass(cell.material));
            let com = bodies[owner].com();
            let center = (
                super::state::cell_center(pos.x),
                super::state::cell_center(pos.y),
            );
            let (cvx, cvy) = cell.vel();
            let exchange = |rel: i64, normal: (i32, i32)| {
                let point = super::contact::body_point_mass(&bodies[owner], com, center, normal);
                let pair = (mass * point / (mass + point)).max(1);
                i128::from(rel) * pair
            };
            let jx = exchange(entrained.0 - i64::from(cvx), (1, 0));
            let jy = exchange(entrained.1 - i64::from(cvy), (0, 1));
            cell.set_vel(cvx + (jx / mass) as i32, cvy + (jy / mass) as i32);
            bodies[owner].apply_impulse(com, pos, -(jx as i64), -(jy as i64));
        }
        displaced.push(cell);
    }
    displaced.sort_unstable_by_key(|cell| std::cmp::Reverse(content::density_milli(cell.material)));
    debug_assert_eq!(vacated.len(), displaced.len());
    for (&pos, &cell) in vacated.iter().zip(&displaced) {
        world.set(pos, cell);
    }
    for (p, cells) in moving.drain(..) {
        let candidate = std::mem::take(&mut proposals[p].candidate);
        for (&pos, &cell) in candidate.iter().zip(&cells) {
            if world.get_cell(pos) != Some(cell) {
                world.set(pos, cell);
            }
        }
        let body = &mut bodies[proposals[p].body];
        apply_commit(body, &proposals[p], candidate);
    }
}

fn entraining_body(
    bodies: &[Body],
    proposals: &[Proposal],
    group: &[usize],
    pos: CellPos,
) -> (usize, (i64, i64)) {
    for &p in group {
        if proposals[p].candidate.contains(&pos) {
            let body = &bodies[proposals[p].body];
            let com = body.com();
            return (proposals[p].body, body.point_velocity(com, pos));
        }
    }
    let body = proposals[group[0]].body;
    (body, (bodies[body].vx, bodies[body].vy))
}

fn apply_commit(body: &mut Body, proposal: &Proposal, candidate: Vec<CellPos>) {
    match proposal.freedom {
        Freedom::X => {
            body.anchor = body
                .anchor
                .translated(proposal.translation.0, proposal.translation.1);
            body.acc_x -= i64::from(proposal.sign) * CELL;
        }
        Freedom::Y => {
            body.anchor = body
                .anchor
                .translated(proposal.translation.0, proposal.translation.1);
            body.acc_y -= i64::from(proposal.sign) * CELL;
        }
        Freedom::Turn => {
            let before = rotated_mean(&body.slots, body.mass, body.step);
            let after = rotated_mean(&body.slots, body.mass, proposal.new_step);
            body.step = proposal.new_step;
            body.acc_turn -= i64::from(proposal.sign) * ORIENTATION_UNITS;
            body.acc_x -= after.0 - before.0;
            body.acc_y -= after.1 - before.1;
        }
    }
    body.raster = candidate;
}

fn finish_round(
    world: &mut CellWorld,
    bodies: &mut [Body],
    movers: &mut [Mover],
    proposals: &mut [Proposal],
    cells: &mut FxHashMap<CellPos, CellState>,
) {
    let mut contacts: Vec<Contact> = Vec::new();
    let mut closing_any: FxHashMap<usize, (bool, i64)> = FxHashMap::default();
    {
        let coms: Vec<(i64, i64)> = bodies.iter().map(Body::com).collect();
        let grounded: Vec<bool> = bodies
            .iter()
            .map(|body| body_grounded(world, body))
            .collect();
        for (p, proposal) in proposals.iter_mut().enumerate() {
            if !proposal.refused {
                continue;
            }
            let approach = proposal.freedom.velocity(&bodies[proposal.body]);
            let resting = proposal.probe;
            let mut any = false;
            proposal.contacts.retain_mut(|contact| {
                let diagonal = contact.normal.0 != 0 && contact.normal.1 != 0;
                let body = &bodies[contact.body];
                let com = coms[contact.body];
                let point = body.point_velocity(com, contact.at);
                let closes_x = point.0 * i64::from(contact.normal.0) < 0;
                let closes_y = point.1 * i64::from(contact.normal.1) < 0;
                let closes = i128::from(point.0) * i128::from(contact.normal.0)
                    + i128::from(point.1) * i128::from(contact.normal.1)
                    < 0;
                if diagonal && !(closes_x && closes_y) {
                    return false;
                }
                if resting {
                    contact.restitution = Q16::from_raw(0);
                }
                any |= closes;
                true
            });
            closing_any.insert(p, (any, approach));
            contacts.append(&mut proposal.contacts);
        }
        contacts.sort_unstable_by_key(|contact| {
            (
                bodies[contact.body].id,
                peer_rank(bodies, &contact.peer),
                contact.at.y,
                contact.at.x,
                contact.normal,
            )
        });
        let mut resolver = Resolver {
            bodies,
            coms: &coms,
            grounded: &grounded,
            cells,
        };
        resolver.resolve(&mut contacts);
    }

    for (p, proposal) in proposals.iter().enumerate() {
        let state = &mut movers[proposal.body].freedoms[proposal.freedom.index()];
        if proposal.probe {
            state.probed = true;
            continue;
        }
        if proposal.canceled {
            let body = &mut bodies[proposal.body];
            *proposal.freedom.accumulator_mut(body) %= proposal.freedom.threshold();
            continue;
        }
        if proposal.refused {
            let (closes, approach) = closing_any.get(&p).copied().unwrap_or((false, 0));
            let body = &mut bodies[proposal.body];
            if closes {
                *proposal.freedom.accumulator_mut(body) -=
                    i64::from(proposal.sign) * proposal.freedom.threshold();
                let rebound = proposal.freedom.velocity(body);
                let accumulator = proposal.freedom.accumulator_mut(body);
                if rebound.signum() as i32 != proposal.sign {
                    let remaining = round_div(
                        i128::from(-*accumulator) * i128::from(rebound.abs()),
                        i128::from(approach.abs().max(1)),
                    ) as i64;
                    *accumulator = remaining.clamp(-MAX_SPEED, MAX_SPEED);
                }
            } else {
                *proposal.freedom.accumulator_mut(body) %= proposal.freedom.threshold();
                state.parked = true;
            }
        }
    }
}

fn peer_rank(bodies: &[Body], peer: &Peer) -> (u8, u64) {
    match peer {
        Peer::Terrain => (0, 0),
        Peer::Body(index) => (1, u64::from(bodies[*index].id)),
        Peer::Cell { pos, .. } => (2, ((pos.y as u32 as u64) << 32) | pos.x as u32 as u64),
    }
}

const STEP_CELLS: i32 = 3;

pub(super) fn body_grounded(world: &CellWorld, body: &Body) -> bool {
    body.raster.iter().any(|&pos| {
        let below = pos.translated(0, -1);
        world.get_cell(below).is_some_and(|cell| {
            cell.body_id().is_none()
                && matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
        })
    })
}

pub(super) fn carriage(world: &mut CellWorld, body: &mut Body) {
    let com = body.com();
    let mut exchanges: Vec<(CellPos, Cell, i64)> = Vec::new();
    for &pos in &body.raster {
        let above = pos.translated(0, 1);
        let Some(cell) = world.get_cell(above) else {
            continue;
        };
        if cell.body_id().is_some() || content::phase(cell.material) != Phase::Powder {
            continue;
        }
        let mass = cell_mass(cell.material);
        let (cvx, cvy) = cell.vel();
        let surface = body.point_velocity(com, above);
        let relative = surface.0 - i64::from(cvx);
        if relative == 0 {
            continue;
        }
        let pair = body.friction.mul(content::friction(cell.material));
        let load = GRAVITY + i64::from((-cvy).max(0));
        let limit = ((i128::from(pair.raw()) * i128::from(load) * i128::from(mass)) >> 16) as i64;
        let effective = super::contact::body_point_mass(
            body,
            com,
            (
                super::state::cell_center(above.x),
                super::state::cell_center(above.y),
            ),
            (1, 0),
        );
        let inertia =
            i128::from(relative) * i128::from(mass) * effective / (i128::from(mass) + effective);
        let impulse = (inertia as i64).clamp(-limit, limit);
        if impulse == 0 {
            continue;
        }
        exchanges.push((above, cell, impulse));
    }
    for (above, cell, impulse) in exchanges {
        let mass = cell_mass(cell.material);
        let (cvx, cvy) = cell.vel();
        let mut written = cell;
        written.set_vel((i64::from(cvx) + impulse / mass) as i32, cvy);
        world.set(above, written);
        body.apply_impulse(com, above, -impulse, 0);
    }
}

pub(super) fn try_settle(world: &CellWorld, body: &mut Body) -> bool {
    if body.parked || !body.settles {
        return false;
    }
    if body.vx.abs() > SNAP || body.vy.abs() > SNAP {
        return false;
    }
    let spin_snap = Spin::for_speed_at(SNAP, body.radius.max(1) * CELL);
    if body.spin.clamped(spin_snap) != body.spin {
        return false;
    }
    let pressing = if net_lift(world, body) > 0 { 1 } else { -1 };
    let mut supports: Vec<(CellPos, CellPos)> = Vec::new();
    for &pos in &body.raster {
        let ahead = pos.translated(0, pressing);
        let Some(cell) = world.get_cell(ahead) else {
            return false;
        };
        if cell.body_id() == Some(body.id) {
            continue;
        }
        if cell.body_id().is_some() {
            return false;
        }
        match content::phase(cell.material) {
            Phase::Solid => supports.push((ahead, pos)),
            Phase::Powder => return false,
            _ => {}
        }
    }
    if supports.is_empty() {
        return false;
    }

    let saved = (body.vx, body.vy, body.spin);
    body.vx = 0;
    body.vy = if pressing > 0 {
        saved.1.max(0) + GRAVITY
    } else {
        saved.1.min(0) - GRAVITY
    };
    body.spin = saved.2;
    let com = body.com();
    let coms = [com];
    let grounded = [false];
    let mut contacts: Vec<Contact> = supports
        .iter()
        .map(|&(at, from)| Contact {
            body: 0,
            from,
            at,
            normal: (0, -pressing),
            peer: Peer::Terrain,
            restitution: Q16::from_raw(0),
            friction: body.friction,
            target: 0,
            push: 0,
            drag: 0,
        })
        .collect();
    let mut cells = FxHashMap::default();
    {
        let slice = std::slice::from_mut(body);
        let mut resolver = Resolver {
            bodies: slice,
            coms: &coms,
            grounded: &grounded,
            cells: &mut cells,
        };
        resolver.resolve(&mut contacts);
    }
    let rests = body.vx.abs() <= SETTLE_EPSILON
        && body.vy.abs() <= SETTLE_EPSILON
        && body.spin.clamped(Spin::for_speed_at(
            SETTLE_EPSILON,
            body.radius.max(1) * CELL,
        )) == body.spin;
    body.vx = saved.0;
    body.vy = saved.1;
    body.spin = saved.2;
    rests
}
