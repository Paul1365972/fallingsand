use super::rotation::Spin;
use super::shape::Vector;
use super::{Body, Peers, Standing};
use fallingsand_core::{Fraction, Subcell};
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
    Stander(u32),
}

#[derive(Clone, Copy)]
pub(super) struct Contact {
    pub axis: Axis,
    pub point: Vector,
    pub restitution: Fraction,
    pub grip: Fraction,
    pub peer: Peer,
    bias: i128,
    push: i128,
    drag: i128,
}

impl Contact {
    pub(super) fn new(
        axis: Axis,
        point: Vector,
        restitution: Fraction,
        grip: Fraction,
        peer: Peer,
    ) -> Self {
        Self {
            axis,
            point,
            restitution,
            grip,
            peer,
            bias: 0,
            push: 0,
            drag: 0,
        }
    }
}

struct Side {
    mass: i64,
    moment: i128,
    velocity: Vector,
    arm: Vector,
}

impl Side {
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

    fn standing(standing: &Standing) -> Self {
        Self {
            mass: standing.mass,
            moment: 0,
            velocity: standing.velocity,
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
    peers: &mut Peers,
) {
    for contact in contacts.iter_mut() {
        let closing = sides(bodies, peers, index, contact, contact.axis.normal())
            .closing(contact.axis.normal());
        contact.bias = if elastic && closing < 0 {
            restitution(bodies, index, contact).scale(-closing)
        } else {
            0
        };
        contact.push = 0;
        contact.drag = 0;
    }

    for _ in 0..ITERATIONS {
        for contact in contacts.iter_mut() {
            let normal = contact.axis.normal();
            let sides = sides(bodies, peers, index, contact, normal);
            let wanted = -(sides.closing(normal) + contact.bias) * sides.effective_mass(normal);
            let total = (contact.push + wanted).max(0);
            apply(bodies, peers, index, contact, normal, total - contact.push);
            contact.push = total;
        }
    }

    for _ in 0..ITERATIONS {
        for contact in contacts.iter_mut() {
            let tangent = contact.axis.normal().perpendicular();
            let sides = sides(bodies, peers, index, contact, tangent);
            let limit = contact.grip.scale(contact.push);
            let wanted = -sides.closing(tangent) * sides.effective_mass(tangent);
            let total = (contact.drag + wanted).clamp(-limit, limit);
            apply(bodies, peers, index, contact, tangent, total - contact.drag);
            contact.drag = total;
        }
    }
}

struct Sides {
    mine: Side,
    theirs: Option<Side>,
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

fn sides(
    bodies: &[Body],
    peers: &Peers,
    index: usize,
    contact: &Contact,
    direction: Vector,
) -> Sides {
    Sides {
        mine: Side::of(&bodies[index], contact.point),
        theirs: match contact.peer {
            Peer::Terrain => None,
            Peer::Body(peer) if peer as usize == index => None,
            Peer::Body(peer) => Some(Side::of(&bodies[peer as usize], contact.point)),
            Peer::Stander(id) => peers
                .standing
                .get(&id)
                .filter(|standing| !standing.braced(direction))
                .map(Side::standing),
        },
    }
}

fn restitution(bodies: &[Body], index: usize, contact: &Contact) -> Fraction {
    let peer = match contact.peer {
        Peer::Body(peer) if peer as usize != index => bodies[peer as usize].restitution,
        _ => Fraction::from_raw(0),
    };
    bodies[index].restitution.max(contact.restitution).max(peer)
}

fn apply(
    bodies: &mut [Body],
    peers: &mut Peers,
    index: usize,
    contact: &Contact,
    direction: Vector,
    magnitude: i128,
) {
    if magnitude == 0 {
        return;
    }
    apply_impulse(&mut bodies[index], contact.point, direction, magnitude);
    match contact.peer {
        Peer::Terrain => {}
        Peer::Body(peer) if peer as usize == index => {}
        Peer::Body(peer) => apply_impulse(
            &mut bodies[peer as usize],
            contact.point,
            direction,
            -magnitude,
        ),
        Peer::Stander(id) => {
            if let Some(standing) = peers.standing.get_mut(&id).filter(|s| !s.braced(direction)) {
                let delta = round_div(-magnitude, i128::from(standing.mass)) as i64;
                standing.velocity.x += delta * direction.x;
                standing.velocity.y += delta * direction.y;
            }
        }
    }
}

pub(super) fn apply_impulse(body: &mut Body, point: Vector, direction: Vector, magnitude: i128) {
    let arm = point.from(Vector::new(body.pose.x.raw(), body.pose.y.raw()));
    let delta = round_div(magnitude, i128::from(body.mass)) as i64;
    body.motion.x += Subcell::from_raw(delta * direction.x);
    body.motion.y += Subcell::from_raw(delta * direction.y);
    body.motion.spin += Spin::from_angular_impulse(magnitude * arm.cross(direction), body.moment);
}
