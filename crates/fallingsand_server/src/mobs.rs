use crate::player::Players;
use crate::regions::ChunkTickets;
use crate::species::{DriveCtx, Mind, Species};
use fallingsand_core::{CellPos, RegionPos};
use fallingsand_math::Hash;
use fallingsand_sim::CellWorld;
use fallingsand_sim::body::{Bodies, Fracture};
use std::collections::BTreeMap;

pub const MAX_MOBS: usize = 64;
const MOB_SALT: Hash = Hash::label("server.mob");
const THREAT_RANGE_X: i32 = 12;
const THREAT_RANGE_Y: i32 = 8;

pub struct Mob {
    pub species: &'static Species,
    pub body_id: u32,
    pub mind: Mind,
    pub hp: f32,
    pub burning_secs: f32,
}

impl Mob {
    pub fn new(species: &'static Species, body_id: u32) -> Self {
        let hp = species.life.as_ref().map_or(0.0, |life| life.max_hp);
        Self {
            species,
            body_id,
            mind: Mind::default(),
            hp,
            burning_secs: 0.0,
        }
    }
}

#[derive(Default)]
pub struct Mobs {
    by_body: BTreeMap<u32, Mob>,
}

impl Mobs {
    pub fn len(&self) -> usize {
        self.by_body.len()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&u32, &mut Mob)> {
        self.by_body.iter_mut()
    }

    pub fn insert(&mut self, mob: Mob) {
        let old = self.by_body.insert(mob.body_id, mob);
        debug_assert!(old.is_none());
    }

    pub fn despawn(&mut self, sim: &mut CellWorld, bodies: &mut Bodies, body_id: u32) {
        if self.by_body.remove(&body_id).is_some() {
            bodies.despawn(sim, body_id);
        }
    }

    pub fn kill(&mut self, sim: &mut CellWorld, bodies: &mut Bodies, body_id: u32) {
        let Some(mob) = self.by_body.remove(&body_id) else {
            return;
        };
        bodies.die(body_id);
        if let Some(life) = &mob.species.life {
            bodies.recast(sim, body_id, life.corpse);
        }
    }

    pub fn resolve_fracture(
        &mut self,
        sim: &mut CellWorld,
        bodies: &mut Bodies,
        fracture: &Fracture,
    ) {
        let Some(mob) = self.by_body.remove(&fracture.source) else {
            return;
        };
        if let Some(life) = &mob.species.life {
            for &part in &fracture.parts {
                bodies.die(part);
                bodies.recast(sim, part, life.corpse);
            }
        }
    }

    pub fn despawn_regions(
        &mut self,
        sim: &mut CellWorld,
        bodies: &mut Bodies,
        regions: &[RegionPos],
    ) {
        let doomed: Vec<u32> = self
            .by_body
            .keys()
            .copied()
            .filter(|&id| {
                bodies.bounds(id).is_some_and(|bounds| {
                    regions.contains(&bounds.min.region()) || regions.contains(&bounds.max.region())
                })
            })
            .collect();
        for body_id in doomed {
            self.despawn(sim, bodies, body_id);
        }
    }
}

pub fn drive_mobs(
    sim: &mut CellWorld,
    players: &Players,
    mobs: &mut Mobs,
    bodies: &mut Bodies,
    tickets: &ChunkTickets,
) {
    let tick = sim.tick();
    let mut doomed = Vec::new();
    for (&body_id, mob) in mobs.iter_mut() {
        let Some(life) = &mob.species.life else {
            continue;
        };
        let Some(bounds) = bodies.bounds(body_id) else {
            doomed.push(body_id);
            continue;
        };
        if !tickets.simulates_rect(bounds) {
            continue;
        }
        let center = CellPos::new(
            (bounds.min.x + bounds.max.x) / 2,
            (bounds.min.y + bounds.max.y) / 2,
        );
        let threat = threat_direction(players, bodies, center);
        let rng = Hash::seed(tick).salt(MOB_SALT).pos(body_id as i32, 0).rng();
        let mut ctx = DriveCtx {
            sim,
            bodies,
            body_id,
            mind: &mut mob.mind,
            threat,
            rng,
        };
        (life.drive)(&mut ctx);
    }
    for body_id in doomed {
        mobs.despawn(sim, bodies, body_id);
    }
}

fn threat_direction(players: &Players, bodies: &Bodies, cell: CellPos) -> Option<i32> {
    let mut best: Option<(i32, i32)> = None;
    for (_, player) in players.iter() {
        if player.avatar().is_none() {
            continue;
        }
        let at = player.view_anchor(bodies);
        let (dx, dy) = (cell.x - at.x, cell.y - at.y);
        if dx.abs() > THREAT_RANGE_X || dy.abs() > THREAT_RANGE_Y {
            continue;
        }
        if best.is_none_or(|(closest, _)| dx.abs() < closest) {
            best = Some((dx.abs(), dx.signum()));
        }
    }
    best.map(|(_, dir)| dir)
}
