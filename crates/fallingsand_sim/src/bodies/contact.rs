use super::{ActorDynamics, PixelBody, Raster};
use crate::world::CellWorld;
use fallingsand_core::content;
use fallingsand_core::{CARDINAL_NEIGHBORS, CellPos, ChunkPos, Phase, Subcell};
use rustc_hash::FxHashMap;

pub(super) type BodyBins = FxHashMap<ChunkPos, Vec<usize>>;

pub(super) enum Other {
    Terrain,
    Entity {
        index: usize,
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
    pub(super) obstruction: CellPos,
    pub(super) orphan: bool,
    pub(super) rx: f32,
    pub(super) ry: f32,
    pub(super) nx: f32,
    pub(super) ny: f32,
    pub(super) restitution: f32,
    pub(super) other: Other,
}

fn entity_contact(entities: &[ActorDynamics], obstruction: CellPos) -> Option<Other> {
    let index = entities
        .iter()
        .position(|entity| entity.bbox.contains_cell(obstruction))?;
    Some(Other::Entity { index })
}

fn classify(
    world: &CellWorld,
    entities: &[ActorDynamics],
    bodies: &[PixelBody],
    body_bins: &BodyBins,
    index: usize,
    obstruction: CellPos,
) -> Option<(Other, f32, bool)> {
    if bodies[index].raster.covers(obstruction) {
        return None;
    }
    if let Some(other_index) = body_bins
        .get(&obstruction.chunk())
        .into_iter()
        .flatten()
        .copied()
        .find(|&other| other != index && bodies[other].covers(obstruction))
    {
        let other = &bodies[other_index];
        return Some((
            Other::Body {
                index: other_index,
                rx: (Subcell::cell_center(obstruction.x) - other.x).to_cells(),
                ry: (Subcell::cell_center(obstruction.y) - other.y).to_cells(),
                resting: other.rest_secs > 0.0,
            },
            other.restitution,
            false,
        ));
    }
    match world.get_cell(obstruction) {
        None => Some((Other::Terrain, 0.0, false)),
        Some(cell)
            if matches!(content::phase(cell.material), Phase::Solid | Phase::Powder)
                && !cell.is_body() =>
        {
            Some((
                Other::Terrain,
                content::material(cell.material).restitution,
                false,
            ))
        }
        Some(cell) => match entity_contact(entities, obstruction) {
            Some(other) => Some((other, 0.0, false)),
            None if cell.is_body() => Some((
                Other::Terrain,
                content::material(cell.material).restitution,
                true,
            )),
            None => None,
        },
    }
}

pub(super) fn find_contacts(
    contacts: &mut Vec<Contact>,
    world: &CellWorld,
    entities: &[ActorDynamics],
    bodies: &[PixelBody],
    body_bins: &BodyBins,
    index: usize,
    raster: &Raster,
) {
    contacts.clear();
    let body = &bodies[index];
    for &(pos, _) in &raster.cells {
        if CARDINAL_NEIGHBORS
            .iter()
            .all(|&(dx, dy)| raster.covers(pos.translated(dx, dy)))
        {
            continue;
        }
        let rx = (Subcell::cell_center(pos.x) - body.x).to_cells();
        let ry = (Subcell::cell_center(pos.y) - body.y).to_cells();
        for (dx, dy) in CARDINAL_NEIGHBORS {
            let obstruction = pos.translated(dx, dy);
            if body.raster.covers(obstruction) {
                continue;
            }
            let Some((other, restitution, orphan)) =
                classify(world, entities, bodies, body_bins, index, obstruction)
            else {
                continue;
            };
            contacts.push(Contact {
                obstruction,
                orphan,
                rx,
                ry,
                nx: -dx as f32,
                ny: -dy as f32,
                restitution,
                other,
            });
        }
    }
}
