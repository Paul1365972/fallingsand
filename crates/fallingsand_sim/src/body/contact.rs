use super::state::{Body, Freedoms, cell_center};
use fallingsand_core::{CellPos, Q16};
use rustc_hash::FxHashMap;

const ITERATIONS: u32 = 8;
const MASS_UNITS: i128 = 1 << 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Peer {
    Terrain,
    Body(usize),
    Cell { pos: CellPos, mass: i64 },
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Contact {
    pub body: usize,
    pub from: CellPos,
    pub at: CellPos,
    pub normal: (i32, i32),
    pub peer: Peer,
    pub restitution: Q16,
    pub friction: Q16,
    pub target: i128,
    pub push: i128,
    pub drag: i128,
}

impl Contact {
    pub(super) fn point(&self) -> (i64, i64) {
        (
            (cell_center(self.from.x) + cell_center(self.at.x)) / 2,
            (cell_center(self.from.y) + cell_center(self.at.y)) / 2,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CellState {
    pub mass: i64,
    pub vx: i64,
    pub vy: i64,
    pub start_vx: i64,
    pub start_vy: i64,
}

pub(super) struct Resolver<'a> {
    pub bodies: &'a mut [Body],
    pub coms: &'a [(i64, i64)],
    pub grounded: &'a [bool],
    pub cells: &'a mut FxHashMap<CellPos, CellState>,
}

struct Side {
    velocity: (i64, i64),
    inverse: Option<Inverse>,
}

struct Inverse {
    mass: i64,
    moment: i128,
    lever: i128,
    rigid_y: bool,
}

impl Side {
    fn effective_mass(&self, normal: (i32, i32)) -> Option<i128> {
        let inverse = self.inverse.as_ref()?;
        let (nx, ny) = (i128::from(normal.0), i128::from(normal.1));
        let translation = nx * nx
            + if inverse.rigid_y && normal.1 != 0 {
                0
            } else {
                ny * ny
            };
        let mass = i128::from(inverse.mass);
        let numerator = mass * inverse.moment;
        let denominator = inverse.moment * translation + mass * inverse.lever * inverse.lever;
        if denominator == 0 {
            return None;
        }
        Some((numerator * MASS_UNITS / denominator).max(1))
    }
}

impl Resolver<'_> {
    fn side_of(&self, contact: &Contact, mine: bool, direction: (i32, i32)) -> Side {
        let point = contact.point();
        if mine {
            let body = &self.bodies[contact.body];
            let com = self.coms[contact.body];
            return body_side(body, com, point, direction, false);
        }
        match contact.peer {
            Peer::Terrain => Side {
                velocity: (0, 0),
                inverse: None,
            },
            Peer::Body(index) => {
                let body = &self.bodies[index];
                let com = self.coms[index];
                let pressing = self.grounded[index] && direction.1 < 0;
                body_side(body, com, point, direction, pressing)
            }
            Peer::Cell { pos, mass } => {
                let state = self.cells[&pos];
                Side {
                    velocity: (state.vx, state.vy),
                    inverse: Some(Inverse {
                        mass,
                        moment: 1,
                        lever: 0,
                        rigid_y: false,
                    }),
                }
            }
        }
    }

    fn closing(&self, contact: &Contact, direction: (i32, i32)) -> i128 {
        let mine = self.side_of(contact, true, direction);
        let theirs = self.side_of(contact, false, (-direction.0, -direction.1));
        let rel = (
            mine.velocity.0 - theirs.velocity.0,
            mine.velocity.1 - theirs.velocity.1,
        );
        i128::from(rel.0) * i128::from(direction.0) + i128::from(rel.1) * i128::from(direction.1)
    }

    fn pair_mass(&self, contact: &Contact, direction: (i32, i32)) -> i128 {
        let mine = self
            .side_of(contact, true, direction)
            .effective_mass(direction);
        let theirs = self
            .side_of(contact, false, (-direction.0, -direction.1))
            .effective_mass(direction);
        match (mine, theirs) {
            (Some(a), Some(b)) => (a * b / (a + b)).max(1),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => 0,
        }
    }

    fn apply(
        &mut self,
        contact: &Contact,
        direction: (i32, i32),
        magnitude: i128,
        energy: &mut i128,
    ) -> i128 {
        if magnitude == 0 {
            return 0;
        }
        let mine = {
            let body = &self.bodies[contact.body];
            (body.vx, body.vy, body.spin)
        };
        let peer_body = match contact.peer {
            Peer::Body(index) => {
                let body = &self.bodies[index];
                Some((index, body.vx, body.vy, body.spin))
            }
            _ => None,
        };
        let peer_cell = match contact.peer {
            Peer::Cell { pos, .. } => self.cells.get(&pos).copied().map(|state| (pos, state)),
            _ => None,
        };
        let before = self.contact_energy(contact);
        let point = contact.point();
        let jx = (magnitude * i128::from(direction.0)) as i64;
        let jy = (magnitude * i128::from(direction.1)) as i64;
        {
            let com = self.coms[contact.body];
            let body = &mut self.bodies[contact.body];
            apply_to_body(body, com, point, jx, jy, false);
        }
        match contact.peer {
            Peer::Terrain => {}
            Peer::Body(index) => {
                let pressing = self.grounded[index] && -direction.1 < 0;
                let com = self.coms[index];
                apply_to_body(&mut self.bodies[index], com, point, -jx, -jy, pressing);
            }
            Peer::Cell { pos, .. } => {
                let state = self.cells.get_mut(&pos).expect("cell state exists");
                state.vx += i128::from(-jx) as i64 / state.mass;
                state.vy += i128::from(-jy) as i64 / state.mass;
            }
        }
        let after = self.contact_energy(contact);
        let candidate = energy.saturating_sub(before).saturating_add(after);
        if candidate <= *energy {
            *energy = candidate;
            return magnitude;
        }
        let body = &mut self.bodies[contact.body];
        (body.vx, body.vy, body.spin) = mine;
        if let Some((index, vx, vy, spin)) = peer_body {
            let body = &mut self.bodies[index];
            (body.vx, body.vy, body.spin) = (vx, vy, spin);
        }
        if let Some((pos, state)) = peer_cell {
            self.cells.insert(pos, state);
        }
        0
    }

    fn kinetic_energy(&self) -> i128 {
        let bodies = self.bodies.iter().map(body_energy);
        let cells = self.cells.values().map(cell_energy);
        bodies.chain(cells).fold(0i128, i128::saturating_add)
    }

    fn contact_energy(&self, contact: &Contact) -> i128 {
        let mine = body_energy(&self.bodies[contact.body]);
        let peer = match contact.peer {
            Peer::Terrain => 0,
            Peer::Body(index) => body_energy(&self.bodies[index]),
            Peer::Cell { pos, .. } => cell_energy(&self.cells[&pos]),
        };
        mine.saturating_add(peer)
    }

    pub(super) fn resolve(&mut self, contacts: &mut [Contact]) {
        let mut energy = self.kinetic_energy();
        for contact in contacts.iter_mut() {
            let closing = self.closing(contact, contact.normal);
            contact.target = if closing < 0 {
                (i128::from(contact.restitution.raw()) * (-closing)) >> 16
            } else {
                0
            };
            contact.push = 0;
            contact.drag = 0;
        }

        for _ in 0..ITERATIONS {
            for contact in contacts.iter_mut() {
                let normal = contact.normal;
                let closing = self.closing(contact, normal);
                let mass = self.pair_mass(contact, normal);
                let wanted = (contact.target - closing) * mass / MASS_UNITS;
                let total = (contact.push + wanted).max(0);
                let delta = total - contact.push;
                contact.push += self.apply(contact, normal, delta, &mut energy);
            }
        }

        for _ in 0..ITERATIONS {
            for contact in contacts.iter_mut() {
                let tangent = (-contact.normal.1, contact.normal.0);
                let closing = self.closing(contact, tangent);
                let mass = self.pair_mass(contact, tangent);
                let limit = (i128::from(contact.friction.raw()) * contact.push) >> 16;
                let wanted = -closing * mass / MASS_UNITS;
                let total = (contact.drag + wanted).clamp(-limit, limit);
                let delta = total - contact.drag;
                contact.drag += self.apply(contact, tangent, delta, &mut energy);
            }
        }
    }
}

fn body_energy(body: &Body) -> i128 {
    let velocity = i128::from(body.vx)
        .saturating_mul(i128::from(body.vx))
        .saturating_add(i128::from(body.vy).saturating_mul(i128::from(body.vy)));
    let radians = super::rotation::RADIANS_PER_TURN;
    let translation = i128::from(body.mass)
        .saturating_mul(velocity)
        .saturating_mul(radians.saturating_mul(radians));
    let angular = i128::from(body.spin.raw()).saturating_mul(super::rotation::TAU_NUMERATOR);
    let rotation = body.moment.saturating_mul(angular.saturating_mul(angular));
    translation.saturating_add(rotation)
}

fn cell_energy(cell: &CellState) -> i128 {
    particle_energy(cell.mass, cell.vx, cell.vy)
}

fn particle_energy(mass: i64, vx: i64, vy: i64) -> i128 {
    let energy = i128::from(mass).saturating_mul(
        i128::from(vx)
            .saturating_mul(i128::from(vx))
            .saturating_add(i128::from(vy).saturating_mul(i128::from(vy))),
    );
    let radians = super::rotation::RADIANS_PER_TURN;
    energy.saturating_mul(radians.saturating_mul(radians))
}

pub(super) fn body_point_mass(
    body: &Body,
    com: (i64, i64),
    point: (i64, i64),
    normal: (i32, i32),
) -> i128 {
    body_side(body, com, point, normal, false)
        .effective_mass(normal)
        .map(|mass| (mass / MASS_UNITS).max(1))
        .unwrap_or(i128::MAX / 2)
}

fn body_side(
    body: &Body,
    com: (i64, i64),
    point: (i64, i64),
    direction: (i32, i32),
    pressing: bool,
) -> Side {
    let rx = i128::from(point.0 - com.0);
    let ry = i128::from(point.1 - com.1);
    let lever = rx * i128::from(direction.1) - ry * i128::from(direction.0);
    Side {
        velocity: (
            body.vx - body.spin.speed_at(point.1 - com.1),
            body.vy + body.spin.speed_at(point.0 - com.0),
        ),
        inverse: Some(Inverse {
            mass: body.mass,
            moment: body.moment,
            lever,
            rigid_y: pressing,
        }),
    }
}

fn apply_to_body(
    body: &mut Body,
    com: (i64, i64),
    point: (i64, i64),
    jx: i64,
    jy: i64,
    pressing: bool,
) {
    let dvx = jx / body.mass;
    let mut dvy = jy / body.mass;
    if pressing && dvy < 0 {
        dvy = 0;
    }
    body.vx += dvx;
    body.vy += dvy;
    let rx = i128::from(point.0 - com.0);
    let ry = i128::from(point.1 - com.1);
    let torque = rx * i128::from(jy) - ry * i128::from(jx);
    if body.holds(Freedoms::TURN) {
        body.spin += super::rotation::Spin::from_angular_impulse(torque, body.moment);
    }
}
