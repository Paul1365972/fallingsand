use super::rotation::quantize_step;
use super::{ActorDynamics, OwnerMap, PixelBody};
use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{CARDINAL_NEIGHBORS, CellPos, Fixed, Phase};

pub(super) enum Other {
    Terrain,
    Entity {
        index: usize,
        inv_mass: f32,
    },
    Body {
        index: usize,
        rx: f32,
        ry: f32,
        resting: bool,
    },
}

impl Other {
    pub(super) const fn is_static(&self) -> bool {
        matches!(self, Other::Terrain | Other::Body { resting: true, .. })
    }
}

pub(super) struct Contact {
    pub(super) rx: f32,
    pub(super) ry: f32,
    pub(super) nx: f32,
    pub(super) ny: f32,
    pub(super) depth: f32,
    pub(super) restitution: f32,
    pub(super) other: Other,
}

fn entity_contact(
    entities: &[ActorDynamics],
    obstruction: CellPos,
    contact: CellPos,
) -> Option<(Other, f32)> {
    let index = entities
        .iter()
        .position(|entity| entity.bbox.contains_cell(obstruction))?;
    let entity = &entities[index];
    let (cx, cy) = (Fixed::cell_center(contact.x), Fixed::cell_center(contact.y));
    let depth_x = entity.bbox.half_w.to_f32() + 0.5 - (cx - entity.bbox.x).to_f32().abs();
    let depth_y = entity.bbox.half_h.to_f32() + 0.5 - (cy - entity.bbox.y).to_f32().abs();
    let depth = depth_x.min(depth_y).clamp(0.5, 4.0);
    Some((
        Other::Entity {
            index,
            inv_mass: entity.inv_mass,
        },
        depth,
    ))
}

fn classify(
    world: &CellWorld,
    entities: &[ActorDynamics],
    bodies: &[PixelBody],
    owners: &OwnerMap,
    index: usize,
    obstruction: CellPos,
    contact: CellPos,
) -> Option<(Other, f32, f32)> {
    if let Some(other_index) = owners.get(obstruction).filter(|&owner| owner != index) {
        let other = &bodies[other_index];
        return Some((
            Other::Body {
                index: other_index,
                rx: (Fixed::cell_center(obstruction.x) - other.x).to_f32(),
                ry: (Fixed::cell_center(obstruction.y) - other.y).to_f32(),
                resting: other.asleep || other.rest_secs > 0.0,
            },
            0.5,
            other.restitution,
        ));
    }
    match world.get_cell(obstruction) {
        None => Some((Other::Terrain, 0.5, 0.0)),
        Some(cell)
            if matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
                && !cell.is_body() =>
        {
            Some((
                Other::Terrain,
                0.5,
                content::material(cell.material).restitution,
            ))
        }
        Some(cell) => match entity_contact(entities, obstruction, contact) {
            Some((other, depth)) => Some((other, depth, 0.0)),
            None if cell.is_body() => Some((
                Other::Terrain,
                0.5,
                content::material(cell.material).restitution,
            )),
            None => None,
        },
    }
}

pub(super) fn find_contacts(
    world: &CellWorld,
    entities: &[ActorDynamics],
    bodies: &[PixelBody],
    owners: &OwnerMap,
    index: usize,
) -> Vec<Contact> {
    let body = &bodies[index];
    let step = quantize_step(body.angle, body.angle_steps);
    let pivot_cell = body.pivot_cell(body.x, body.y);
    let mut contacts: Vec<Contact> = Vec::new();
    for &(lx, ly) in &body.perimeter {
        let pos = body.body_cell(pivot_cell, step, lx, ly);
        let rx = (Fixed::cell_center(pos.x) - body.x).to_f32();
        let ry = (Fixed::cell_center(pos.y) - body.y).to_f32();
        for (dx, dy) in CARDINAL_NEIGHBORS {
            let obstruction = pos.translated(dx, dy);
            if body.raster.covers(obstruction) {
                continue;
            }
            let Some((other, depth, restitution)) =
                classify(world, entities, bodies, owners, index, obstruction, pos)
            else {
                continue;
            };
            contacts.push(Contact {
                rx,
                ry,
                nx: -dx as f32,
                ny: -dy as f32,
                depth,
                restitution,
                other,
            });
        }
    }
    contacts
}
