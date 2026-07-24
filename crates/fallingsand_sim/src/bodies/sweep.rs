use super::contact::{Axis, Contact, Peer, resolve};
use super::rotation::{Spin, TURN_UNITS};
use super::shape::{Motion, Pose, Vector, rasterize};
use super::{Ambient, Body, Peers, Standing};
use crate::motion::MAX_SPEED_CELLS;
use crate::world::{CellWorld, blocking};
use fallingsand_core::{CellPos, Subcell, content};
use fallingsand_math::{SUBCELL_UNITS_PER_CELL, ceil_div, round_div, ticks_from_secs};

const CELL: i64 = SUBCELL_UNITS_PER_CELL as i64;
const MAX_SPEED: i64 = MAX_SPEED_CELLS as i64 * CELL;
const MAX_SUBSTEPS: u32 = 96;
const WHOLE_TICK: i64 = 1 << 16;
const REST_TICKS: u32 = ticks_from_secs(0.5) as u32;

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

pub(super) fn advance(
    world: &CellWorld,
    bodies: &mut [Body],
    index: usize,
    peers: &mut Peers,
    ambient: &Ambient<'_>,
    scratch: &mut Scratch,
) -> Advance {
    {
        let body = &mut bodies[index];
        body.motion.y += ambient.gravity;
        body.motion = capped(body.motion, body.radius);
        scratch.current.clone_from(&body.raster);
    }

    let mut unspent = WHOLE_TICK;
    let mut substeps = 0;
    let mut elastic = true;
    let mut frozen = false;

    while unspent > 0 && substeps < MAX_SUBSTEPS {
        let pass = sweep(
            world,
            &mut bodies[index],
            index as u32,
            peers,
            ambient,
            unspent,
            scratch,
        );
        substeps += pass.steps;
        match pass.stop {
            Stop::Spent => break,
            Stop::Frontier => {
                frozen = true;
                break;
            }
            Stop::Blocked => {
                unspent -= round_div(
                    i128::from(unspent) * i128::from(pass.consumed),
                    i128::from(pass.steps),
                ) as i64;
                let before = bodies[index].motion;
                resolve(bodies, index, &mut scratch.contacts, elastic, peers);
                elastic = false;
                if bodies[index].motion == before {
                    break;
                }
            }
        }
    }

    let body = &mut bodies[index];
    body.pose.angle = body.pose.angle.rem_euclid(TURN_UNITS);
    if frozen {
        body.motion = Motion::default();
    }
    let moved = scratch.current != body.raster;
    body.rest = if moved || frozen { 0 } else { body.rest + 1 };
    Advance {
        moved,
        settled: body.rest >= REST_TICKS,
    }
}

struct Pass {
    steps: u32,
    consumed: u32,
    stop: Stop,
}

enum Stop {
    Spent,
    Blocked,
    Frontier,
}

fn sweep(
    world: &CellWorld,
    body: &mut Body,
    index: u32,
    peers: &mut Peers,
    ambient: &Ambient<'_>,
    unspent: i64,
    scratch: &mut Scratch,
) -> Pass {
    let motion = body.motion.part(unspent, WHOLE_TICK);
    if motion.is_still() {
        return Pass {
            steps: 0,
            consumed: 0,
            stop: Stop::Spent,
        };
    }
    let steps = substeps(motion, body.radius);
    let mut consumed = 0;
    let (mut x, mut y, mut angle) = (0i128, 0i128, 0i128);
    for _ in 0..steps {
        let next = Pose {
            x: body.pose.x + Subcell::from_raw(split(&mut x, motion.x.raw(), steps)),
            y: body.pose.y + Subcell::from_raw(split(&mut y, motion.y.raw(), steps)),
            angle: body.pose.angle + split(&mut angle, motion.spin.raw(), steps),
        };
        rasterize(&body.slots, body.mass, next, &mut scratch.candidate);
        if scratch.candidate == scratch.current {
            body.pose = next;
            consumed += 1;
            continue;
        }
        match probe(
            world,
            index,
            peers,
            ambient,
            &scratch.current,
            &scratch.candidate,
            &mut scratch.contacts,
        ) {
            Probe::Free => {
                body.pose = next;
                consumed += 1;
                std::mem::swap(&mut scratch.current, &mut scratch.candidate);
            }
            Probe::Blocked => {
                return Pass {
                    steps,
                    consumed,
                    stop: Stop::Blocked,
                };
            }
            Probe::Frontier => {
                return Pass {
                    steps,
                    consumed,
                    stop: Stop::Frontier,
                };
            }
        }
    }
    Pass {
        steps,
        consumed,
        stop: Stop::Spent,
    }
}

enum Probe {
    Free,
    Blocked,
    Frontier,
}

fn probe(
    world: &CellWorld,
    index: u32,
    peers: &mut Peers,
    ambient: &Ambient<'_>,
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
            content::surface_grip(cell.material),
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

fn split(remainder: &mut i128, motion: i64, steps: u32) -> i64 {
    *remainder += i128::from(motion);
    let step = *remainder / i128::from(steps);
    *remainder %= i128::from(steps);
    step as i64
}
