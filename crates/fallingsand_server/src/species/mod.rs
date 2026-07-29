pub mod ball;
pub mod balloon;
pub mod flesh;
mod frame;
pub mod frog;

pub use frame::Frame;

use crate::mobs::{MAX_MOBS, Mob, Mobs};
use crate::player::BodyIds;
use fallingsand_core::{CellPos, MaterialId, Subcell};
use fallingsand_math::Rng;
use fallingsand_sim::CellWorld;
use fallingsand_sim::body::{Bodies, Policy};

pub const SPECIES: &[&Species] = &[&frog::SPECIES, &ball::SPECIES, &balloon::SPECIES];

pub fn find(name: &str) -> Option<&'static Species> {
    SPECIES.iter().copied().find(|species| species.name == name)
}

pub struct Species {
    pub name: &'static str,
    pub frame: &'static Frame,
    pub policy: Policy,
    pub life: Option<Life>,
}

pub struct Life {
    pub max_hp: f32,
    pub corpse: MaterialId,
    pub drive: fn(&mut DriveCtx),
}

pub struct Mind {
    pub rest: u16,
    pub facing: i32,
}

impl Default for Mind {
    fn default() -> Self {
        Self {
            rest: 30,
            facing: 1,
        }
    }
}

pub struct DriveCtx<'a> {
    pub sim: &'a mut CellWorld,
    pub bodies: &'a mut Bodies,
    pub body_id: u32,
    pub mind: &'a mut Mind,
    pub threat: Option<i32>,
    pub rng: Rng,
}

impl DriveCtx<'_> {
    pub fn velocity(&self) -> (Subcell, Subcell) {
        self.bodies
            .velocity(self.body_id)
            .unwrap_or((Subcell::ZERO, Subcell::ZERO))
    }

    pub fn supported(&self) -> bool {
        self.bodies.supported(self.sim, self.body_id)
    }

    pub fn submersion(&self) -> f32 {
        self.bodies.submersion(self.sim, self.body_id)
    }

    pub fn drive(&mut self, vx: Subcell, vy: Subcell) {
        self.bodies.drive(self.body_id, vx, vy);
    }

    pub fn paint(&mut self, frame: &Frame, facing_left: bool) {
        self.bodies.repaint(self.sim, self.body_id, |dx, dy| {
            frame.shade(dx, dy, facing_left)
        });
    }
}

pub fn summon(
    sim: &mut CellWorld,
    bodies: &mut Bodies,
    mobs: &mut Mobs,
    body_ids: &mut BodyIds,
    species: &'static Species,
    anchor: CellPos,
    facing: i32,
) -> Result<(), &'static str> {
    if species.life.is_some() && mobs.len() >= MAX_MOBS {
        return Err("too many mobs");
    }
    let width = species.frame.width();
    let body_id = body_ids.allocate();
    for dy in [1, 0, 2, 3] {
        for dist in [4, 5, 6, 3] {
            let x = if facing < 0 {
                anchor.x - dist - (width - 1)
            } else {
                anchor.x + dist
            };
            let cells = species
                .frame
                .cells(CellPos::new(x, anchor.y + dy), facing < 0);
            if !bodies.spawn(sim, body_id, &cells, species.policy) {
                continue;
            }
            if species.life.is_some() {
                mobs.insert(Mob::new(species, body_id));
            }
            return Ok(());
        }
    }
    Err("no room to summon here")
}
