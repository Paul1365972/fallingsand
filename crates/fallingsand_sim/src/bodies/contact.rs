use super::{ActorDynamics, PixelBody, quantized_trig};
use crate::world::CellWorld;
use fallingsand_core::{CellPos, MaterialRegistry, Phase};
use rustc_hash::FxHashSet;

pub(super) enum Other {
    Terrain,
    Entity {
        index: usize,
        inv_mass: f32,
        vx: f32,
        vy: f32,
    },
    Body {
        index: usize,
        inv_mass: f32,
        inv_inertia: f32,
        vx: f32,
        vy: f32,
        spin: f32,
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

fn obstructed(
    world: &CellWorld,
    registry: &MaterialRegistry,
    entities: &[ActorDynamics],
    own: &FxHashSet<CellPos>,
    pos: CellPos,
) -> bool {
    if own.contains(&pos) {
        return false;
    }
    let solid = match world.get_cell(pos) {
        Some(cell) => matches!(
            registry.get(cell.material).phase,
            Phase::Solid | Phase::Powder
        ),
        None => true,
    };
    solid || entities.iter().any(|entity| entity.bbox.contains_cell(pos))
}

pub(super) fn find_contacts(
    world: &CellWorld,
    registry: &MaterialRegistry,
    entities: &[ActorDynamics],
    bodies: &[PixelBody],
    index: usize,
) -> Vec<Contact> {
    let body = &bodies[index];
    let (sin, cos) = quantized_trig(body.angle);
    let mut contacts: Vec<Contact> = Vec::new();
    for &(lx, ly) in &body.perimeter {
        let (ox, oy) = body.offset_with(sin, cos, lx as f32 + 0.5, ly as f32 + 0.5);
        let (wx, wy) = (body.x.add_f32(ox), body.y.add_f32(oy));
        let pos = CellPos::new(wx.floor_cell(), wy.floor_cell());
        if body.raster.covers(pos) {
            continue;
        }

        let mut depth = 0.5;
        let mut surface = 0.0f32;
        let other = match world.get_cell(pos) {
            None => Other::Terrain,
            Some(cell) if cell.is_body() => {
                let owner = bodies
                    .iter()
                    .position(|other| other.id != body.id && other.raster.covers(pos));
                match owner {
                    Some(other_index) => {
                        let other = &bodies[other_index];
                        surface = other.restitution;
                        Other::Body {
                            index: other_index,
                            inv_mass: other.inv_mass,
                            inv_inertia: other.inv_inertia,
                            vx: other.vx.vel_f32(),
                            vy: other.vy.vel_f32(),
                            spin: other.spin,
                            rx: (wx - other.x).to_f32(),
                            ry: (wy - other.y).to_f32(),
                            resting: other.asleep || other.rest_secs > 0.0,
                        }
                    }
                    None => {
                        let player = entities
                            .iter()
                            .position(|entity| entity.bbox.contains_cell(pos));
                        match player {
                            Some(entity_index) => {
                                let entity = &entities[entity_index];
                                let depth_x = entity.bbox.half_w.to_f32() + 0.5
                                    - (wx - entity.bbox.x).to_f32().abs();
                                let depth_y = entity.bbox.half_h.to_f32() + 0.5
                                    - (wy - entity.bbox.y).to_f32().abs();
                                depth = depth_x.min(depth_y).clamp(0.5, 4.0);
                                Other::Entity {
                                    index: entity_index,
                                    inv_mass: entity.inv_mass,
                                    vx: entity.vx,
                                    vy: entity.vy,
                                }
                            }
                            None => {
                                surface = registry.get(cell.material).restitution;
                                Other::Terrain
                            }
                        }
                    }
                }
            }
            Some(cell)
                if matches!(
                    registry.get(cell.material).phase,
                    Phase::Solid | Phase::Powder
                ) =>
            {
                surface = registry.get(cell.material).restitution;
                Other::Terrain
            }
            Some(_) => {
                let Some(entity_index) = entities
                    .iter()
                    .position(|entity| entity.bbox.contains_cell(pos))
                else {
                    continue;
                };
                let entity = &entities[entity_index];
                let depth_x =
                    entity.bbox.half_w.to_f32() + 0.5 - (wx - entity.bbox.x).to_f32().abs();
                let depth_y =
                    entity.bbox.half_h.to_f32() + 0.5 - (wy - entity.bbox.y).to_f32().abs();
                depth = depth_x.min(depth_y).clamp(0.5, 4.0);
                Other::Entity {
                    index: entity_index,
                    inv_mass: entity.inv_mass,
                    vx: entity.vx,
                    vy: entity.vy,
                }
            }
        };

        let mut nx = 0.0f32;
        let mut ny = 0.0f32;
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                if (dx, dy) == (0, 0) {
                    continue;
                }
                if !obstructed(
                    world,
                    registry,
                    entities,
                    &body.raster.set,
                    pos.translated(dx, dy),
                ) {
                    nx += dx as f32;
                    ny += dy as f32;
                }
            }
        }
        let length = (nx * nx + ny * ny).sqrt();
        let (nx, ny) = if length > 1e-3 {
            (nx / length, ny / length)
        } else {
            (0.0, 1.0)
        };
        contacts.push(Contact {
            rx: ox,
            ry: oy,
            nx,
            ny,
            depth,
            restitution: surface,
            other,
        });
    }
    contacts
}
