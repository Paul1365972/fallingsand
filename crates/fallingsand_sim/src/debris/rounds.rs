use super::body::{CELL, Debris, cell_mass, rasterize, rotated_mean};
use super::contact::{CellState, Contact, CreatureState, Peer, Resolver};
use super::rotation::{ANGLE_STEPS, ORIENTATION_UNITS, Spin};
use crate::motion::{GRAVITY_DV, MAX_SPEED_CELLS, SETTLE};
use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, ChunkPos, MaterialId, Phase, Q16, content};
use rustc_hash::{FxHashMap, FxHashSet};

const GRAVITY: i64 = GRAVITY_DV as i64;
const MAX_BODY_SPEED_CELLS: i64 = 6;
const MAX_SPEED: i64 = MAX_BODY_SPEED_CELLS * CELL;
const _: () = assert!(MAX_BODY_SPEED_CELLS <= MAX_SPEED_CELLS as i64);
const MAX_TURN_QUANTA: i64 = 32;
const DRAG_DIVISOR: i64 = 4;
const SNAP: i64 = 16;
const SETTLE_EPSILON: i64 = 4;

pub(super) type CreatureLookup<'a> = &'a dyn Fn(u32) -> Option<super::CreaturePeer>;

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

    fn threshold(self) -> i64 {
        match self {
            Self::Turn => ORIENTATION_UNITS,
            Self::X | Self::Y => CELL,
        }
    }

    fn accumulator(self, body: &Debris) -> i64 {
        match self {
            Self::Turn => body.acc_turn,
            Self::X => body.acc_x,
            Self::Y => body.acc_y,
        }
    }

    fn accumulator_mut(self, body: &mut Debris) -> &mut i64 {
        match self {
            Self::Turn => &mut body.acc_turn,
            Self::X => &mut body.acc_x,
            Self::Y => &mut body.acc_y,
        }
    }

    fn velocity(self, body: &Debris) -> i64 {
        match self {
            Self::Turn => body.spin.raw(),
            Self::X => body.vx,
            Self::Y => body.vy,
        }
    }

    fn pending(self, body: &Debris) -> i64 {
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

pub(super) fn integrate_forces(world: &CellWorld, body: &mut Debris) {
    let com = body.com();
    let own = body.id;
    let mut impulse = (0i128, 0i128);
    let mut torque = 0i128;
    let mut wetted: Vec<(CellPos, i64, i64, i64)> = Vec::new();
    let mut weights: Vec<(CellPos, i128)> = Vec::with_capacity(body.slots.len());
    let mut lift = 0i128;
    let mut lift_above = 0i128;
    for (slot, &pos) in body.slots.iter().zip(&body.raster) {
        let mass = cell_mass(slot.material);
        let mut ambient = 0i64;
        for (dx, dy) in fallingsand_core::CARDINAL_NEIGHBORS {
            let near = pos.translated(dx, dy);
            let Some(cell) = world.get_cell(near) else {
                continue;
            };
            if cell.body_id() == Some(own) {
                continue;
            }
            match content::phase(cell.material) {
                Phase::Liquid | Phase::Gas | Phase::Empty => {
                    let density = i64::from(content::density_milli(cell.material).max(1));
                    ambient = ambient.max(density);
                    if content::phase(cell.material) == Phase::Liquid {
                        let (cvx, cvy) = cell.vel();
                        let point = body.point_velocity(com, pos);
                        wetted.push((
                            pos,
                            i64::from(cvx) - point.0,
                            i64::from(cvy) - point.1,
                            density,
                        ));
                    }
                }
                _ => {}
            }
        }
        let weight = i128::from(-GRAVITY * (mass - ambient));
        weights.push((pos, weight));
        lift += weight;
        lift_above += i128::from(-GRAVITY * (mass - ambient_at(world, own, pos.translated(0, 1))));
    }
    let floating = !wetted.is_empty() && lift > 0 && lift_above <= 0;
    if floating {
        body.vy /= 2;
    } else {
        for (pos, weight) in weights {
            impulse.1 += weight;
            let rx = i128::from(super::body::cell_center(pos.x) - com.0);
            torque += rx * weight;
        }
    }
    let touch_share = body.mass / i64::try_from(wetted.len().max(1)).expect("touch count fits");
    for (pos, rel_x, rel_y, density) in wetted {
        let coupled = density.min(touch_share);
        let jx = i128::from(rel_x) * i128::from(coupled) / i128::from(DRAG_DIVISOR);
        let jy = i128::from(rel_y) * i128::from(coupled) / i128::from(DRAG_DIVISOR);
        impulse.0 += jx;
        impulse.1 += jy;
        let rx = i128::from(super::body::cell_center(pos.x) - com.0);
        let ry = i128::from(super::body::cell_center(pos.y) - com.1);
        torque += rx * jy - ry * jx;
    }
    body.vx += fallingsand_math::round_div(impulse.0, i128::from(body.mass)) as i64;
    body.vy += fallingsand_math::round_div(impulse.1, i128::from(body.mass)) as i64;
    body.spin += Spin::from_angular_impulse(torque, body.moment);
    body.vx = body.vx.clamp(-MAX_SPEED, MAX_SPEED);
    body.vy = body.vy.clamp(-MAX_SPEED, MAX_SPEED);
    let turn_cap = Spin::for_speed_at(MAX_SPEED, body.radius.max(1) * CELL)
        .clamped(Spin::from_raw(MAX_TURN_QUANTA * ORIENTATION_UNITS));
    body.spin = body.spin.clamped(turn_cap);
}

fn ambient_at(world: &CellWorld, own: u32, pos: CellPos) -> i64 {
    let mut ambient = 0i64;
    for (dx, dy) in fallingsand_core::CARDINAL_NEIGHBORS {
        let near = pos.translated(dx, dy);
        let Some(cell) = world.get_cell(near) else {
            continue;
        };
        if cell.body_id() == Some(own) {
            continue;
        }
        if matches!(
            content::phase(cell.material),
            Phase::Liquid | Phase::Gas | Phase::Empty
        ) {
            ambient = ambient.max(i64::from(content::density_milli(cell.material).max(1)));
        }
    }
    ambient
}

pub(super) fn run_rounds<S>(
    world: &mut CellWorld,
    bodies: &mut [Debris],
    by_id: &FxHashMap<u32, usize>,
    simulated: &S,
    creature: CreatureLookup,
    creatures: &mut FxHashMap<u32, CreatureState>,
    cells: &mut FxHashMap<CellPos, CellState>,
) where
    S: Fn(ChunkPos) -> bool,
{
    let floor = i64::from(SETTLE);
    let mut movers = vec![Mover::default(); bodies.len()];
    for body in bodies.iter_mut() {
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
            creature,
            creatures,
            cells,
            &mut movers,
            &mut proposals,
            &mut newly_parked,
        );
        for index in newly_parked {
            bodies[index].parked = true;
        }
        resolve_ties(bodies, &mut proposals);
        cascade(bodies, &mut proposals);
        commit(world, bodies, &mut proposals, by_id, creatures, cells);
        finish_round(world, bodies, &mut movers, &mut proposals, creatures, cells);
    }
}

fn collect_proposals(world: &CellWorld, bodies: &[Debris], movers: &mut [Mover]) -> Vec<Proposal> {
    let mut proposals = Vec::new();
    for (index, body) in bodies.iter().enumerate() {
        if body.parked {
            continue;
        }
        let mover = &movers[index];
        let mut best: Option<Freedom> = None;
        for freedom in Freedom::ALL {
            if mover.freedoms[freedom.index()].parked || freedom.pending(body) == 0 {
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
            if state.parked || state.probed || freedom.velocity(body) == 0 {
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
    bodies: &[Debris],
    index: usize,
    freedom: Freedom,
    sign: i32,
    probe: bool,
) -> Proposal {
    let body = &bodies[index];
    let mut candidate = Vec::new();
    let mut entered = Vec::new();
    let mut new_step = body.step;
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
    bodies: &[Debris],
    by_id: &FxHashMap<u32, usize>,
    simulated: &S,
    creature: CreatureLookup,
    creatures: &mut FxHashMap<u32, CreatureState>,
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
                                Peer::Debris(peer_index),
                                pair_restitution(my, Some(peer), cell.material),
                                my.friction.mul(peer.friction),
                            );
                        }
                    } else if let Some(peer) = creature(id) {
                        creatures.entry(id).or_insert(CreatureState {
                            mass: peer.mass_milli,
                            vx: peer.vx.raw(),
                            vy: peer.vy.raw(),
                            start_vx: peer.vx.raw(),
                            start_vy: peer.vy.raw(),
                            grounded: peer.grounded,
                        });
                        push_contact(
                            &mut proposals[p],
                            body_index,
                            at,
                            from,
                            Peer::Creature(id),
                            pair_restitution(my, None, cell.material),
                            my.friction.mul(content::friction(cell.material)),
                        );
                    } else {
                        push_contact(
                            &mut proposals[p],
                            body_index,
                            at,
                            from,
                            Peer::Terrain,
                            pair_restitution(my, None, cell.material),
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
                            pair_restitution(my, None, cell.material),
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
                            pair_restitution(my, None, cell.material),
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

fn pair_restitution(body: &Debris, peer: Option<&Debris>, material: MaterialId) -> Q16 {
    let mut restitution = body.restitution;
    let material_restitution = content::restitution(material);
    if material_restitution.raw() > restitution.raw() {
        restitution = material_restitution;
    }
    if let Some(peer) = peer
        && peer.restitution.raw() > restitution.raw()
    {
        restitution = peer.restitution;
    }
    restitution
}

fn powder_peer(
    body: &Debris,
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
    let needed = closing.max(0) * i128::from(body.mass.min(mass * 64));
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

fn resolve_ties(bodies: &[Debris], proposals: &mut [Proposal]) {
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
            Peer::Debris(winner_body),
            pair_restitution(my, Some(peer), my.slots[0].material),
            my.friction.mul(peer.friction),
        );
        proposals[p].refused = true;
    }
}

fn cascade(bodies: &[Debris], proposals: &mut [Proposal]) {
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
                    Peer::Debris(peer_body),
                    pair_restitution(my, Some(peer), my.slots[0].material),
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
    bodies: &mut [Debris],
    proposals: &mut [Proposal],
    by_id: &FxHashMap<u32, usize>,
    creatures: &mut FxHashMap<u32, CreatureState>,
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
                refuse_after_commit(world, bodies, by_id, creatures, cells, proposals, p);
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
                refuse_after_commit(world, bodies, by_id, creatures, cells, proposals, p);
            }
        }
    }
}

fn revalidate(world: &CellWorld, bodies: &[Debris], proposal: &Proposal) -> bool {
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
    bodies: &[Debris],
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
    bodies: &[Debris],
    by_id: &FxHashMap<u32, usize>,
    creatures: &mut FxHashMap<u32, CreatureState>,
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
                        Peer::Debris(peer_index),
                        pair_restitution(my, Some(peer), cell.material),
                        my.friction.mul(peer.friction),
                    );
                } else if creatures.contains_key(&id) {
                    push_contact(
                        &mut proposals[p],
                        body_index,
                        at,
                        from,
                        Peer::Creature(id),
                        pair_restitution(my, None, cell.material),
                        my.friction.mul(content::friction(cell.material)),
                    );
                } else {
                    push_contact(
                        &mut proposals[p],
                        body_index,
                        at,
                        from,
                        Peer::Terrain,
                        pair_restitution(my, None, cell.material),
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
                    pair_restitution(my, None, cell.material),
                    my.friction.mul(content::friction(cell.material)),
                );
            }
        }
    }
    proposals[p].refused = true;
}

fn commit_group(
    world: &mut CellWorld,
    bodies: &mut [Debris],
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
                super::body::cell_center(pos.x),
                super::body::cell_center(pos.y),
            );
            let (cvx, cvy) = cell.vel();
            let exchange = |rel: i64, normal: (i32, i32)| {
                let point = super::contact::debris_point_mass(&bodies[owner], com, center, normal);
                let pair = (mass * point / (mass + point)).max(1);
                i128::from(rel) * pair
            };
            let jx = exchange(entrained.0 - i64::from(cvx), (1, 0));
            let jy = exchange(entrained.1 - i64::from(cvy), (0, 1));
            cell.set_vel(
                cvx + fallingsand_math::round_div(jx, mass) as i32,
                cvy + fallingsand_math::round_div(jy, mass) as i32,
            );
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
    bodies: &[Debris],
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

fn apply_commit(body: &mut Debris, proposal: &Proposal, candidate: Vec<CellPos>) {
    match proposal.freedom {
        Freedom::X => {
            body.anchor = body.anchor.translated(proposal.sign, 0);
            body.acc_x -= i64::from(proposal.sign) * CELL;
        }
        Freedom::Y => {
            body.anchor = body.anchor.translated(0, proposal.sign);
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
    bodies: &mut [Debris],
    movers: &mut [Mover],
    proposals: &mut [Proposal],
    creatures: &mut FxHashMap<u32, CreatureState>,
    cells: &mut FxHashMap<CellPos, CellState>,
) {
    let mut contacts: Vec<Contact> = Vec::new();
    let mut closing_any: FxHashMap<usize, (bool, i64)> = FxHashMap::default();
    {
        let coms: Vec<(i64, i64)> = bodies.iter().map(Debris::com).collect();
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
            creatures,
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
        if proposal.refused {
            let (closes, approach) = closing_any.get(&p).copied().unwrap_or((false, 0));
            if closes {
                let body = &mut bodies[proposal.body];
                let rebound = proposal.freedom.velocity(body);
                let accumulator = proposal.freedom.accumulator_mut(body);
                *accumulator -= i64::from(proposal.sign) * proposal.freedom.threshold();
                if rebound.signum() as i32 != proposal.sign {
                    let remaining = fallingsand_math::round_div(
                        i128::from(-*accumulator) * i128::from(rebound.abs()),
                        i128::from(approach.abs().max(1)),
                    ) as i64;
                    *accumulator = remaining.clamp(-MAX_SPEED, MAX_SPEED);
                }
            } else {
                state.parked = true;
            }
        }
    }
}

fn peer_rank(bodies: &[Debris], peer: &Peer) -> (u8, u64) {
    match peer {
        Peer::Terrain => (0, 0),
        Peer::Debris(index) => (1, u64::from(bodies[*index].id)),
        Peer::Creature(id) => (2, u64::from(*id)),
        Peer::Cell { pos, .. } => (3, ((pos.y as u32 as u64) << 32) | pos.x as u32 as u64),
    }
}

pub(super) fn body_grounded(world: &CellWorld, body: &Debris) -> bool {
    body.raster.iter().any(|&pos| {
        let below = pos.translated(0, -1);
        world.get_cell(below).is_some_and(|cell| {
            cell.body_id().is_none()
                && matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
        })
    })
}

pub(super) fn carriage(world: &mut CellWorld, body: &mut Debris) {
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
        let effective = super::contact::debris_point_mass(
            body,
            com,
            (
                super::body::cell_center(above.x),
                super::body::cell_center(above.y),
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

pub(super) fn try_settle(world: &CellWorld, body: &mut Debris) -> bool {
    if body.parked {
        return false;
    }
    if body.vx.abs() > SNAP || body.vy.abs() > SNAP {
        return false;
    }
    let spin_snap = Spin::for_speed_at(SNAP, body.radius.max(1) * CELL);
    if body.spin.clamped(spin_snap) != body.spin {
        return false;
    }
    let mut supports: Vec<(CellPos, CellPos)> = Vec::new();
    for &pos in &body.raster {
        let below = pos.translated(0, -1);
        let Some(cell) = world.get_cell(below) else {
            return false;
        };
        if cell.body_id() == Some(body.id) {
            continue;
        }
        if cell.body_id().is_some() {
            return false;
        }
        match content::phase(cell.material) {
            Phase::Solid => supports.push((below, pos)),
            Phase::Powder => return false,
            _ => {}
        }
    }
    if supports.is_empty() {
        return false;
    }

    let saved = (body.vx, body.vy, body.spin);
    body.vx = 0;
    body.vy = saved.1.min(0) - GRAVITY;
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
            normal: (0, 1),
            peer: Peer::Terrain,
            restitution: Q16::from_raw(0),
            friction: body.friction,
            target: 0,
            push: 0,
            drag: 0,
        })
        .collect();
    let mut creatures = FxHashMap::default();
    let mut cells = FxHashMap::default();
    {
        let slice = std::slice::from_mut(body);
        let mut resolver = Resolver {
            bodies: slice,
            coms: &coms,
            grounded: &grounded,
            creatures: &mut creatures,
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
