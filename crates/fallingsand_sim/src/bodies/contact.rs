use super::rotation::Spin;
use super::shape::Vector;
use super::{Body, Shove, Stander};
use fallingsand_core::{CellPos, Subcell, VelocityFactor};
use fallingsand_math::round_div;

const ITERATIONS: u32 = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Axis {
    PosX,
    NegX,
    PosY,
    NegY,
}

impl Axis {
    pub(super) fn entering(dx: i32, dy: i32) -> Option<Self> {
        if dx.abs() >= dy.abs() && dx != 0 {
            Some(if dx > 0 { Self::NegX } else { Self::PosX })
        } else if dy != 0 {
            Some(if dy > 0 { Self::NegY } else { Self::PosY })
        } else {
            None
        }
    }

    fn normal(self) -> Vector {
        match self {
            Self::PosX => Vector::new(1, 0),
            Self::NegX => Vector::new(-1, 0),
            Self::PosY => Vector::new(0, 1),
            Self::NegY => Vector::new(0, -1),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Peer {
    Terrain,
    Body(u32),
    Stander(Stander),
}

#[derive(Clone, Copy)]
pub(super) struct Contact {
    pub cell: CellPos,
    pub axis: Axis,
    pub point: Vector,
    pub restitution: VelocityFactor,
    pub grip: VelocityFactor,
    pub peer: Peer,
    bias: i128,
    push: i128,
    drag: i128,
    reaction: Vector,
}

impl Contact {
    pub(super) fn new(
        cell: CellPos,
        axis: Axis,
        point: Vector,
        restitution: VelocityFactor,
        grip: VelocityFactor,
        peer: Peer,
    ) -> Self {
        Self {
            cell,
            axis,
            point,
            restitution,
            grip,
            peer,
            bias: 0,
            push: 0,
            drag: 0,
            reaction: Vector::new(0, 0),
        }
    }
}

struct Party {
    mass: i64,
    moment: i128,
    velocity: Vector,
    arm: Vector,
}

impl Party {
    fn of(body: &Body, point: Vector) -> Self {
        let arm = point.from(Vector::new(body.pose.x.raw(), body.pose.y.raw()));
        Self {
            mass: body.mass,
            moment: body.moment,
            velocity: Vector::new(
                body.motion.x.raw() - body.motion.spin.speed_at(arm.y),
                body.motion.y.raw() + body.motion.spin.speed_at(arm.x),
            ),
            arm,
        }
    }

    fn standing(stander: Stander) -> Self {
        Self {
            mass: stander.mass,
            moment: 0,
            velocity: Vector::new(stander.vx.raw(), stander.vy.raw()),
            arm: Vector::new(0, 0),
        }
    }

    fn effective_mass(&self, direction: Vector) -> i128 {
        let mass = i128::from(self.mass);
        if self.moment == 0 {
            return mass;
        }
        let lever = self.arm.cross(direction);
        (mass * self.moment / (self.moment + mass * lever * lever)).max(1)
    }
}

pub(super) fn resolve(
    bodies: &mut [Body],
    index: usize,
    contacts: &mut [Contact],
    elastic: bool,
    shoves: &mut Vec<Shove>,
) {
    for contact in contacts.iter_mut() {
        let closing = sides(bodies, index, contact).closing(contact.axis.normal());
        contact.bias = if elastic && closing < 0 {
            restitution(bodies, index, contact).scale(-closing)
        } else {
            0
        };
        contact.push = 0;
        contact.drag = 0;
        contact.reaction = Vector::new(0, 0);
    }

    for _ in 0..ITERATIONS {
        for contact in contacts.iter_mut() {
            let normal = contact.axis.normal();
            let sides = sides(bodies, index, contact);
            let wanted = -(sides.closing(normal) + contact.bias) * sides.effective_mass(normal);
            let total = (contact.push + wanted).max(0);
            apply(bodies, index, contact, normal, total - contact.push);
            contact.push = total;
        }
    }

    for _ in 0..ITERATIONS {
        for contact in contacts.iter_mut() {
            let tangent = contact.axis.normal().perpendicular();
            let sides = sides(bodies, index, contact);
            let limit = contact.grip.scale(contact.push);
            let wanted = -sides.closing(tangent) * sides.effective_mass(tangent);
            let total = (contact.drag + wanted).clamp(-limit, limit);
            apply(bodies, index, contact, tangent, total - contact.drag);
            contact.drag = total;
        }
    }

    for contact in contacts.iter() {
        let Peer::Stander(stander) = contact.peer else {
            continue;
        };
        let mass = i128::from(stander.mass);
        let dvx = round_div(i128::from(contact.reaction.x), mass) as i64;
        let dvy = round_div(i128::from(contact.reaction.y), mass) as i64;
        if dvx != 0 || dvy != 0 {
            shoves.push(Shove {
                pos: contact.cell,
                dvx: Subcell::from_raw(dvx),
                dvy: Subcell::from_raw(dvy),
            });
        }
    }
}

struct Sides {
    mine: Party,
    theirs: Option<Party>,
}

impl Sides {
    fn closing(&self, direction: Vector) -> i128 {
        let theirs = self
            .theirs
            .as_ref()
            .map_or(Vector::new(0, 0), |party| party.velocity);
        self.mine.velocity.from(theirs).dot(direction)
    }

    fn effective_mass(&self, direction: Vector) -> i128 {
        let mine = self.mine.effective_mass(direction);
        match &self.theirs {
            Some(theirs) => {
                let theirs = theirs.effective_mass(direction);
                (mine * theirs / (mine + theirs)).max(1)
            }
            None => mine,
        }
    }
}

fn sides(bodies: &[Body], index: usize, contact: &Contact) -> Sides {
    Sides {
        mine: Party::of(&bodies[index], contact.point),
        theirs: match contact.peer {
            Peer::Terrain => None,
            Peer::Body(peer) if peer as usize == index => None,
            Peer::Body(peer) => Some(Party::of(&bodies[peer as usize], contact.point)),
            Peer::Stander(stander) => Some(Party::standing(stander)),
        },
    }
}

fn restitution(bodies: &[Body], index: usize, contact: &Contact) -> VelocityFactor {
    let peer = match contact.peer {
        Peer::Body(peer) if peer as usize != index => bodies[peer as usize].restitution,
        _ => VelocityFactor::from_raw(0),
    };
    bodies[index].restitution.max(contact.restitution).max(peer)
}

fn apply(
    bodies: &mut [Body],
    index: usize,
    contact: &mut Contact,
    direction: Vector,
    magnitude: i128,
) {
    if magnitude == 0 {
        return;
    }
    drive(&mut bodies[index], contact.point, direction, magnitude);
    match contact.peer {
        Peer::Terrain => {}
        Peer::Body(peer) if peer as usize == index => {}
        Peer::Body(peer) => drive(
            &mut bodies[peer as usize],
            contact.point,
            direction,
            -magnitude,
        ),
        Peer::Stander(_) => {
            contact.reaction.x -= (magnitude * i128::from(direction.x)) as i64;
            contact.reaction.y -= (magnitude * i128::from(direction.y)) as i64;
        }
    }
}

pub(super) fn drive(body: &mut Body, point: Vector, direction: Vector, magnitude: i128) {
    let arm = point.from(Vector::new(body.pose.x.raw(), body.pose.y.raw()));
    let delta = round_div(magnitude, i128::from(body.mass)) as i64;
    body.motion.x += Subcell::from_raw(delta * direction.x);
    body.motion.y += Subcell::from_raw(delta * direction.y);
    body.motion.spin += Spin::from_angular_impulse(magnitude * arm.cross(direction), body.moment);
}
