use super::contact::{Axis, Contact, Peer, resolve};
use super::rotation::{Spin, TURN_UNITS};
use super::shape::{Motion, Pose, Vector, rasterize};
use super::{Ambient, Body, Peers, Stander, Standing};
use crate::motion::MAX_SPEED_CELLS;
use crate::world::{CellWorld, blocking};
use fallingsand_core::{CellPos, ChunkPos, Subcell, content};
use fallingsand_math::{SUBCELL_UNITS_PER_CELL, ceil_div, round_div, ticks_from_secs};

const CELL: i64 = SUBCELL_UNITS_PER_CELL as i64;
const MAX_SPEED: i64 = MAX_SPEED_CELLS as i64 * CELL;
const MAX_SUBSTEPS: u32 = 96;
const WHOLE_TICK: i64 = 1 << 16;
const REST_TICKS: u32 = ticks_from_secs(1.5) as u32;

#[derive(Default)]
pub(super) struct Scratch {
    pub current: Vec<CellPos>,
    pub candidate: Vec<CellPos>,
    pub contacts: Vec<Contact>,
}

pub(super) struct Advance {
    pub moved: bool,
    pub settled: bool,
}

#[derive(Clone, Copy)]
enum Freedom {
    Turn,
    X,
    Y,
}

impl Freedom {
    const ALL: [Self; 3] = [Self::Turn, Self::X, Self::Y];

    fn of(self, motion: Motion) -> i64 {
        match self {
            Self::Turn => motion.spin.raw(),
            Self::X => motion.x.raw(),
            Self::Y => motion.y.raw(),
        }
    }

    fn shifted(self, pose: Pose, delta: i64) -> Pose {
        match self {
            Self::Turn => Pose {
                angle: pose.angle + delta,
                ..pose
            },
            Self::X => Pose {
                x: pose.x + Subcell::from_raw(delta),
                ..pose
            },
            Self::Y => Pose {
                y: pose.y + Subcell::from_raw(delta),
                ..pose
            },
        }
    }

    fn absorb(self, motion: &mut Motion) {
        match self {
            Self::Turn => motion.spin = Spin::ZERO,
            Self::X => motion.x = Subcell::ZERO,
            Self::Y => motion.y = Subcell::ZERO,
        }
    }
}

pub(super) fn advance<S: Fn(ChunkPos) -> bool, P: Fn(CellPos) -> Option<Stander>>(
    world: &CellWorld,
    bodies: &mut [Body],
    index: usize,
    peers: &mut Peers,
    ambient: &Ambient<S, P>,
    scratch: &mut Scratch,
) -> Advance {
    {
        let body = &mut bodies[index];
        body.motion.y += ambient.gravity;
        body.motion = capped(body.motion, body.radius);
        scratch.current.clone_from(&body.raster);
    }

    let mut unspent = WHOLE_TICK;
    let mut elastic = true;
    let mut frozen = false;
    let mut touched = false;

    'tick: for _ in 0..MAX_SUBSTEPS {
        let remaining = bodies[index].motion.part(unspent, WHOLE_TICK);
        if unspent <= 0 || remaining.is_still() {
            break;
        }
        let budget = unspent / substeps(remaining, bodies[index].radius) as i64;
        for freedom in Freedom::ALL {
            let body = &bodies[index];
            let motion = body.motion.part(unspent, WHOLE_TICK);
            let steps = substeps(motion, body.radius);
            let delta = round_div(i128::from(freedom.of(motion)), i128::from(steps)) as i64;
            if delta == 0 {
                continue;
            }
            let next = freedom.shifted(body.pose, delta);
            rasterize(&body.slots, body.mass, next, &mut scratch.candidate);
            if scratch.candidate == scratch.current {
                bodies[index].pose = next;
                continue;
            }
            match probe(
                world,
                index as u32,
                peers,
                ambient,
                &scratch.current,
                &scratch.candidate,
                &mut scratch.contacts,
            ) {
                Probe::Free => {
                    bodies[index].pose = next;
                    std::mem::swap(&mut scratch.current, &mut scratch.candidate);
                }
                Probe::Frontier => {
                    frozen = true;
                    break 'tick;
                }
                Probe::Blocked => {
                    touched = true;
                    let before = freedom.of(bodies[index].motion);
                    resolve(bodies, index, &mut scratch.contacts, elastic, peers);
                    elastic = false;
                    if freedom.of(bodies[index].motion) == before {
                        freedom.absorb(&mut bodies[index].motion);
                    }
                }
            }
        }
        unspent -= budget.max(1);
    }

    if !touched {
        touched = lean(world, bodies, index, peers, ambient, scratch);
    }

    let body = &mut bodies[index];
    body.pose.angle = body.pose.angle.rem_euclid(TURN_UNITS);
    if frozen {
        body.motion = Motion::default();
    }
    let moved = scratch.current != body.raster;
    body.rest = if touched && !moved && !frozen {
        body.rest + 1
    } else {
        0
    };
    Advance {
        moved,
        settled: body.rest >= REST_TICKS,
    }
}

fn lean<S: Fn(ChunkPos) -> bool, P: Fn(CellPos) -> Option<Stander>>(
    world: &CellWorld,
    bodies: &mut [Body],
    index: usize,
    peers: &mut Peers,
    ambient: &Ambient<S, P>,
    scratch: &mut Scratch,
) -> bool {
    let body = &bodies[index];
    let below = Pose {
        y: body.pose.y + ambient.gravity.signum_cell(),
        ..body.pose
    };
    rasterize(&body.slots, body.mass, below, &mut scratch.candidate);
    if scratch.candidate == scratch.current {
        return false;
    }
    match probe(
        world,
        index as u32,
        peers,
        ambient,
        &scratch.current,
        &scratch.candidate,
        &mut scratch.contacts,
    ) {
        Probe::Blocked => {
            resolve(bodies, index, &mut scratch.contacts, false, peers);
            true
        }
        _ => false,
    }
}

enum Probe {
    Free,
    Blocked,
    Frontier,
}

fn probe<S: Fn(ChunkPos) -> bool, P: Fn(CellPos) -> Option<Stander>>(
    world: &CellWorld,
    index: u32,
    peers: &mut Peers,
    ambient: &Ambient<S, P>,
    current: &[CellPos],
    candidate: &[CellPos],
    contacts: &mut Vec<Contact>,
) -> Probe {
    contacts.clear();
    for (slot, &pos) in candidate.iter().enumerate() {
        let from = current[slot];
        let owned = peers.owner_of(pos);
        if pos == from || owned == Some(index) {
            continue;
        }
        if !(ambient.simulated)(pos.chunk()) {
            return Probe::Frontier;
        }
        let Some(cell) = world.get_cell(pos) else {
            return Probe::Frontier;
        };
        if !blocking(cell) {
            continue;
        }
        let Some(axis) = Axis::entering(pos.x - from.x, pos.y - from.y) else {
            continue;
        };
        let peer = match owned {
            Some(peer) => Peer::Body(peer),
            None => match (ambient.stander)(pos) {
                Some(stander) => {
                    peers
                        .standing
                        .entry(stander.id)
                        .or_insert_with(|| Standing::new(stander));
                    Peer::Stander(stander.id)
                }
                None => Peer::Terrain,
            },
        };
        contacts.push(Contact::new(
            axis,
            Vector::of_cell(from).midpoint(Vector::of_cell(pos)),
            content::restitution(cell.material),
            content::friction(cell.material),
            peer,
        ));
    }
    if contacts.is_empty() {
        Probe::Free
    } else {
        Probe::Blocked
    }
}

fn substeps(motion: Motion, radius: i64) -> u32 {
    let cells = |speed: i64| ceil_div(i128::from(speed.abs()), i128::from(CELL)) as i64;
    cells(motion.x.raw())
        .max(cells(motion.y.raw()))
        .max(cells(motion.spin.speed_at(radius * CELL)))
        .max(motion.spin.orientations())
        .max(1) as u32
}

fn capped(motion: Motion, radius: i64) -> Motion {
    let turning = Spin::for_speed_at(MAX_SPEED, radius * CELL).clamped(Spin::from_raw(TURN_UNITS));
    Motion {
        x: Subcell::from_raw(motion.x.raw().clamp(-MAX_SPEED, MAX_SPEED)),
        y: Subcell::from_raw(motion.y.raw().clamp(-MAX_SPEED, MAX_SPEED)),
        spin: motion.spin.clamped(turning),
    }
}
