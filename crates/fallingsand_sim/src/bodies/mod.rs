mod contact;
mod grid;
mod island;
mod rotation;
mod shape;
mod sweep;

pub use island::detect_detached_island;
pub use shape::material_mass;

use crate::world::CellWorld;
use fallingsand_core::{Cell, CellPos, ChunkPos, Fraction, RegionPos, Subcell, content};
use grid::{capture, release, split};
use rustc_hash::FxHashMap;
use shape::{Motion, Pose, Slot, Vector};

const UNCLAIMED: u32 = u32::MAX;

#[derive(Clone, Copy)]
pub struct Stander {
    pub id: u32,
    pub mass: i64,
    pub vx: Subcell,
    pub vy: Subcell,
    pub grounded: bool,
}

pub struct Shove {
    pub id: u32,
    pub dvx: Subcell,
    pub dvy: Subcell,
}

struct Standing {
    mass: i64,
    start: Vector,
    velocity: Vector,
    grounded: bool,
}

impl Standing {
    fn new(stander: Stander) -> Self {
        let velocity = Vector::new(stander.vx.raw(), stander.vy.raw());
        Self {
            mass: stander.mass,
            start: velocity,
            velocity,
            grounded: stander.grounded,
        }
    }

    pub(super) fn braced(&self, direction: Vector) -> bool {
        self.grounded && direction.y > 0
    }

    fn gained(&self) -> Vector {
        self.velocity.from(self.start)
    }
}

pub struct Ambient<S, P> {
    pub gravity: Subcell,
    pub simulated: S,
    pub stander: P,
}

#[derive(Default)]
struct Peers {
    owner: FxHashMap<CellPos, u32>,
    standing: FxHashMap<u32, Standing>,
}

impl Peers {
    fn owner_of(&self, pos: CellPos) -> Option<u32> {
        self.owner.get(&pos).copied()
    }

    fn claim(&mut self, index: usize, raster: &[CellPos]) {
        for &pos in raster {
            self.owner.insert(pos, index as u32);
        }
    }

    fn disown(&mut self, raster: &[CellPos]) {
        for &pos in raster {
            self.owner.remove(&pos);
        }
    }
}

#[derive(Default)]
pub struct BodySet {
    bodies: Vec<Body>,
    peers: Peers,
    shoves: Vec<Shove>,
    sweep: sweep::Scratch,
    split: grid::Scratch,
    relocate: RelocateScratch,
}

#[derive(Default)]
struct RelocateScratch {
    cells: Vec<Cell>,
    displaced: Vec<Cell>,
    vacated: Vec<CellPos>,
}

pub(crate) struct Body {
    slots: Vec<Slot>,
    raster: Vec<CellPos>,
    pose: Pose,
    motion: Motion,
    mass: i64,
    moment: i128,
    radius: i64,
    restitution: Fraction,
    rest: u32,
}

impl BodySet {
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    pub fn rasters(&self) -> impl Iterator<Item = impl Iterator<Item = CellPos> + '_> + '_ {
        self.bodies.iter().map(|body| body.raster.iter().copied())
    }

    pub fn drain_shoves(&mut self) -> impl Iterator<Item = Shove> + '_ {
        self.shoves.drain(..)
    }

    pub fn detach(&mut self, world: &mut CellWorld, island: Vec<CellPos>) {
        let body = capture(world, island);
        self.peers.claim(self.bodies.len(), &body.raster);
        self.bodies.push(body);
    }

    pub fn push(&mut self, pos: CellPos, dvx: Subcell, dvy: Subcell, mass: i64) -> bool {
        let Some(index) = self.peers.owner_of(pos) else {
            return false;
        };
        let body = &mut self.bodies[index as usize];
        let momentum = i128::from(body.mass.min(mass.max(1)));
        let point = Vector::of_cell(pos);
        contact::apply_impulse(
            body,
            point,
            Vector::new(1, 0),
            i128::from(dvx.raw()) * momentum,
        );
        contact::apply_impulse(
            body,
            point,
            Vector::new(0, 1),
            i128::from(dvy.raw()) * momentum,
        );
        body.rest = 0;
        true
    }

    pub fn reconcile(&mut self, world: &mut CellWorld) {
        let mut index = 0;
        while index < self.bodies.len() {
            let Some(mut parts) = split(world, &self.bodies[index], &mut self.split) else {
                index += 1;
                continue;
            };
            match parts.pop() {
                Some(survivor) => {
                    self.bodies[index] = survivor;
                    self.bodies.append(&mut parts);
                    index += 1;
                }
                None => {
                    self.bodies.swap_remove(index);
                }
            }
        }
    }

    pub fn step<S: Fn(ChunkPos) -> bool, P: Fn(CellPos) -> Option<Stander>>(
        &mut self,
        world: &mut CellWorld,
        ambient: &Ambient<S, P>,
    ) {
        self.bodies
            .sort_unstable_by_key(|body| body.raster.iter().map(|pos| pos.y).min());
        self.peers.owner.clear();
        self.peers.standing.clear();
        for index in 0..self.bodies.len() {
            let raster = self.bodies[index].raster.clone();
            self.peers.claim(index, &raster);
        }
        let mut index = 0;
        while index < self.bodies.len() {
            let advance = sweep::advance(
                world,
                &mut self.bodies,
                index,
                &mut self.peers,
                ambient,
                &mut self.sweep,
            );
            if advance.moved {
                relocate(
                    world,
                    &mut self.bodies[index],
                    index as u32,
                    &mut self.peers,
                    &self.sweep.current,
                    &mut self.relocate,
                );
            }
            if advance.settled {
                let body = self.remove(index);
                settle(world, &body);
            } else {
                index += 1;
            }
        }
        for (&id, standing) in &self.peers.standing {
            let gained = standing.gained();
            if gained != Vector::new(0, 0) {
                self.shoves.push(Shove {
                    id,
                    dvx: Subcell::from_raw(gained.x),
                    dvy: Subcell::from_raw(gained.y),
                });
            }
        }
    }

    pub fn settle_regions(&mut self, world: &mut CellWorld, regions: &[RegionPos]) {
        let mut index = 0;
        while index < self.bodies.len() {
            let overlaps = self.bodies[index].raster.iter().any(|cell| {
                regions
                    .iter()
                    .any(|&region| cell.chunk().region() == region)
            });
            if overlaps {
                let body = self.remove(index);
                settle(world, &body);
            } else {
                index += 1;
            }
        }
    }

    fn remove(&mut self, index: usize) -> Body {
        let body = self.bodies.swap_remove(index);
        self.peers.disown(&body.raster);
        if index < self.bodies.len() {
            let moved = self.bodies[index].raster.clone();
            self.peers.claim(index, &moved);
        }
        body
    }
}

impl Body {
    fn center(&self) -> Vector {
        Vector::new(self.pose.x.raw(), self.pose.y.raw())
    }

    fn point_velocity(&self, pos: CellPos) -> (i64, i64) {
        let arm = Vector::of_cell(pos).from(self.center());
        (
            self.motion.x.raw() - self.motion.spin.speed_at(arm.y),
            self.motion.y.raw() + self.motion.spin.speed_at(arm.x),
        )
    }
}

fn relocate(
    world: &mut CellWorld,
    body: &mut Body,
    index: u32,
    peers: &mut Peers,
    raster: &[CellPos],
    scratch: &mut RelocateScratch,
) {
    scratch.cells.clear();
    scratch.cells.extend(
        body.raster
            .iter()
            .map(|&pos| world.get_cell(pos).expect("body raster is loaded")),
    );
    scratch.displaced.clear();
    scratch.displaced.extend(
        raster
            .iter()
            .filter(|pos| peers.owner_of(**pos) != Some(index))
            .map(|&pos| world.get_cell(pos).expect("body proposal is loaded")),
    );
    scratch
        .displaced
        .sort_unstable_by_key(|cell| std::cmp::Reverse(content::density_milli(cell.material)));

    peers.disown(&body.raster);
    peers.claim(index as usize, raster);
    scratch.vacated.clear();
    scratch.vacated.extend(
        body.raster
            .iter()
            .copied()
            .filter(|pos| peers.owner_of(*pos) != Some(index)),
    );
    scratch.vacated.sort_unstable_by_key(|pos| (pos.y, pos.x));
    debug_assert_eq!(scratch.vacated.len(), scratch.displaced.len());

    for (&pos, &cell) in scratch.vacated.iter().zip(&scratch.displaced) {
        world.set(pos, cell, true);
    }
    for (&pos, &cell) in raster.iter().zip(&scratch.cells) {
        if world.get_cell(pos) != Some(cell) {
            world.set(pos, cell, true);
        }
    }
    body.raster.clear();
    body.raster.extend_from_slice(raster);
}

fn settle(world: &mut CellWorld, body: &Body) {
    for &pos in &body.raster {
        if world.get_cell(pos).is_some_and(|cell| cell.is_body()) {
            release(world, body, pos);
        }
    }
}
