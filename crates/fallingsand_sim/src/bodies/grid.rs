use super::shape::{Motion, Pose, Slot, Vector, center_of, frame, rotated_mean};
use super::{Body, UNCLAIMED};
use crate::world::{CellWorld, mobile};
use fallingsand_core::{CARDINAL_NEIGHBORS, CellPos, MaterialId, Subcell, content};
use rustc_hash::FxHashMap;
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Scratch {
    live: Vec<Option<MaterialId>>,
    keep: Vec<usize>,
    slots: Vec<Slot>,
    by_local: FxHashMap<(i32, i32), u32>,
    part: Vec<u32>,
    queue: VecDeque<u32>,
}

pub(super) fn capture(world: &mut CellWorld, raster: Vec<CellPos>) -> Body {
    let count = raster.len() as i64;
    let anchor = CellPos::new(
        (raster.iter().map(|pos| i64::from(pos.x)).sum::<i64>() / count) as i32,
        (raster.iter().map(|pos| i64::from(pos.y)).sum::<i64>() / count) as i32,
    );
    let slots: Vec<Slot> = raster
        .iter()
        .map(|&pos| Slot {
            local: (pos.x - anchor.x, pos.y - anchor.y),
            material: world.get_cell(pos).expect("body island is loaded").material,
        })
        .collect();
    let frame = frame(&slots);
    let (x, y) = center_of(&raster, &slots, frame.mass);
    for &pos in &raster {
        let mut cell = world.get_cell(pos).expect("body island is loaded");
        cell.set_body(true);
        world.set(pos, cell, false);
    }

    Body {
        slots,
        raster,
        pose: Pose { x, y, angle: 0 },
        motion: Motion::default(),
        mass: frame.mass,
        moment: frame.moment,
        radius: frame.radius,
        restitution: frame.restitution,
        rest: 0,
    }
}

fn derive(source: &Body, slots: Vec<Slot>, raster: Vec<CellPos>) -> Body {
    let before = rotated_mean(&source.slots, source.mass, source.pose.angle);
    let frame = frame(&slots);
    let after = rotated_mean(&slots, frame.mass, source.pose.angle);
    let shift = Vector::new(after.0 - before.0, after.1 - before.1);
    Body {
        slots,
        raster,
        pose: Pose {
            x: source.pose.x + Subcell::from_raw(shift.x),
            y: source.pose.y + Subcell::from_raw(shift.y),
            angle: source.pose.angle,
        },
        motion: Motion {
            x: Subcell::from_raw(source.motion.x.raw() - source.motion.spin.speed_at(shift.y)),
            y: Subcell::from_raw(source.motion.y.raw() + source.motion.spin.speed_at(shift.x)),
            spin: source.motion.spin,
        },
        mass: frame.mass,
        moment: frame.moment,
        radius: frame.radius,
        restitution: frame.restitution,
        rest: 0,
    }
}

pub(super) fn split(
    world: &mut CellWorld,
    body: &Body,
    scratch: &mut Scratch,
) -> Option<Vec<Body>> {
    scratch.live.clear();
    let mut intact = true;
    for (&pos, slot) in body.raster.iter().zip(&body.slots) {
        match world.get_cell(pos) {
            Some(cell) if cell.is_body() => {
                intact &= cell.material == slot.material;
                scratch.live.push(Some(cell.material));
            }
            _ => {
                intact = false;
                scratch.live.push(None);
            }
        }
    }
    if intact {
        return None;
    }

    for (&pos, live) in body.raster.iter().zip(&scratch.live) {
        if live.is_none() {
            release(world, body, pos);
        }
    }

    scratch.keep.clear();
    scratch.slots.clear();
    for (index, live) in scratch.live.iter().enumerate() {
        if let Some(material) = *live {
            scratch.keep.push(index);
            scratch.slots.push(Slot {
                local: body.slots[index].local,
                material,
            });
        }
    }
    scratch.by_local.clear();
    for (slot, entry) in scratch.slots.iter().enumerate() {
        scratch.by_local.insert(entry.local, slot as u32);
    }
    scratch.part.clear();
    scratch.part.resize(scratch.slots.len(), UNCLAIMED);

    let parts = flood(scratch);
    let mut bodies = Vec::new();
    for part in 0..parts {
        let mut slots = Vec::new();
        let mut raster = Vec::new();
        for (slot, &index) in scratch.keep.iter().enumerate() {
            if scratch.part[slot] == part {
                slots.push(scratch.slots[slot]);
                raster.push(body.raster[index]);
            }
        }
        bodies.push(derive(body, slots, raster));
    }
    for (slot, &index) in scratch.keep.iter().enumerate() {
        if scratch.part[slot] == UNCLAIMED {
            release(world, body, body.raster[index]);
        }
    }
    Some(bodies)
}

fn flood(scratch: &mut Scratch) -> u32 {
    let mut parts = 0;
    for start in 0..scratch.slots.len() {
        if scratch.part[start] != UNCLAIMED || !scratch.slots[start].bonded() {
            continue;
        }
        scratch.part[start] = parts;
        scratch.queue.push_back(start as u32);
        while let Some(current) = scratch.queue.pop_front() {
            let slot = scratch.slots[current as usize];
            for next in neighbours(scratch, slot.local).into_iter().flatten() {
                let neighbour = scratch.slots[next as usize];
                if scratch.part[next as usize] == UNCLAIMED
                    && neighbour.bonded()
                    && content::bonds(slot.material, neighbour.material)
                {
                    scratch.part[next as usize] = parts;
                    scratch.queue.push_back(next);
                }
            }
        }
        parts += 1;
    }

    scratch.queue.extend(
        (0..scratch.slots.len() as u32).filter(|&slot| scratch.part[slot as usize] != UNCLAIMED),
    );
    while let Some(current) = scratch.queue.pop_front() {
        let slot = scratch.slots[current as usize];
        let part = scratch.part[current as usize];
        for next in neighbours(scratch, slot.local).into_iter().flatten() {
            if scratch.part[next as usize] == UNCLAIMED {
                scratch.part[next as usize] = part;
                scratch.queue.push_back(next);
            }
        }
    }
    parts
}

fn neighbours(scratch: &Scratch, local: (i32, i32)) -> [Option<u32>; 4] {
    CARDINAL_NEIGHBORS.map(|(dx, dy)| scratch.by_local.get(&(local.0 + dx, local.1 + dy)).copied())
}

pub(super) fn release(world: &mut CellWorld, body: &Body, pos: CellPos) {
    let Some(mut cell) = world.get_cell(pos).filter(|cell| !cell.is_air()) else {
        return;
    };
    let (vx, vy) = if mobile(cell.material) {
        body.point_velocity(pos)
    } else {
        (0, 0)
    };
    cell.set_vel(vx as i32, vy as i32);
    cell.set_body(false);
    world.set(pos, cell, false);
}
