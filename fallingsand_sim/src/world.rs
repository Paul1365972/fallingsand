use std::hash::{Hash, Hasher};

use crate::chunk::{EntityEntry, EntityKey, Region, UnloadedRegion};
use crate::chunk_tickets::{ChunkTicketField, ChunkTicketKey};
use crate::entity::entity::MyEntityVariant;
use crate::util::coords::{CellCoords, WorldRegionCoords};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHasher};

#[derive(Default)]
pub struct World {
    regions: FxHashMap<WorldRegionCoords, Region>,
    region_tickets: ChunkTicketField,
    entities: FxHashMap<EntityKey, EntityEntry>,
    context: GlobalContext,
}

pub struct PlayerInputState {
    id: u32, // ???
    pressed_keys: u16,
}

pub struct GlobalContext {
    pub ticks: u32,
}

impl Default for GlobalContext {
    fn default() -> Self {
        Self { ticks: 0 }
    }
}

impl GlobalContext {
    pub fn next_u64(&self, coords: CellCoords) -> u64 {
        let mut hasher = FxHasher::default();
        self.ticks.hash(&mut hasher);
        coords.hash(&mut hasher);
        hasher.finish()
    }
}

impl World {
    pub fn load_region(&mut self, coords: WorldRegionCoords, region: UnloadedRegion) {
        // for entity in chunk.entities {
        //     self.entities.insert(entity.key, entity);
        // }
        self.regions.insert(
            coords.clone(),
            Region {
                chunks: region.tile_chunk,
                entity_keys: Default::default(),
                simulation_cells: None,
                num_neighbors: 0,
            },
        );

        let mut num_neighbors = 0;
        for coords in coords.neighbors_exclusive().into_iter() {
            if let Some(neighbor_region) = self.regions.get_mut(&coords) {
                num_neighbors += 1;
                neighbor_region.num_neighbors += 1;

                if neighbor_region.num_neighbors == 8 {
                    let neighbors = coords.neighbors_inclusive();
                    let keys = std::array::from_fn(|i| &neighbors[i]);
                    Region::initalize_simulation_cells(self.regions.get_many_mut(keys).unwrap())
                }
            }
        }
        let region = self.regions.get_mut(&coords).unwrap();
        region.num_neighbors = num_neighbors;
        if region.num_neighbors == 8 {
            let neighbors = coords.neighbors_inclusive();
            let keys = std::array::from_fn(|i| &neighbors[i]);
            Region::initalize_simulation_cells(self.regions.get_many_mut(keys).unwrap())
        }
    }

    pub fn unload_region(&mut self, coords: &WorldRegionCoords) -> UnloadedRegion {
        let region = self.regions.remove(coords).unwrap();
        for coords in coords.neighbors_exclusive().into_iter() {
            if let Some(neighbor_region) = self.regions.get_mut(&coords) {
                neighbor_region.simulation_cells = None;
                neighbor_region.num_neighbors -= 1;
            }
        }
        // Handle entity unloads
        UnloadedRegion {
            tile_chunk: region.chunks,
            entities: vec![],
        }
    }
}

impl World {
    pub fn step_context(&mut self) {
        self.context.ticks += 1;
    }

    pub fn step(&mut self, players: Vec<PlayerInputState>) {
        // pre: already received network packets
        // send network packets
        self.step_context();
        self.step_tiles();
        self.step_entities();
    }

    pub fn step_tiles(&mut self) {
        for offset_index in 0..4 {
            self.regions
                .par_iter_mut()
                .filter_map(|(_, region)| region.simulation_cells.as_mut())
                .flat_map(|cells| &mut cells[offset_index])
                .for_each(|cell| {
                    let mut cell = cell.promote();
                    cell.step(&self.context);
                });
        }
        // To do maybe collect tile events here and apply them
    }

    pub fn step_entities(&mut self) {
        let mut events = Vec::new();
        // Do step entity stuff here

        for (key, value) in self.entities.iter_mut() {
            value.entity.step();
        }

        // Remove entities
        self.entities.retain(|key, value| {
            let entity = &mut value.entity;
            if entity.should_remove() {
                self.regions
                    .get_mut(&value.chunk_coords)
                    .unwrap()
                    .entity_keys
                    .remove(key);
                if let MyEntityVariant::Player(Some(ticket_key)) = &entity.variant {
                    events.push(EntityStepResult::ChunkTicketRemoved(ticket_key.clone()));
                }
                return false;
            }
            true
        });

        // Move entities
        for (key, value) in self.entities.iter_mut() {
            let entity = &mut value.entity;
            let offset = entity.apply_move();
            if offset != (0, 0) {
                self.regions
                    .get_mut(&value.chunk_coords)
                    .unwrap()
                    .entity_keys
                    .remove(&key);
                let new_coords = &value.chunk_coords + offset;
                self.regions
                    .get_mut(&new_coords)
                    .unwrap()
                    .entity_keys
                    .insert(*key);

                if let MyEntityVariant::Player(Some(ticket_key)) = &entity.variant {
                    events.push(EntityStepResult::ChunkTicketMoved(
                        ticket_key.clone(),
                        offset,
                    ));
                }
            }
        }

        // Deferred updates
        // events.into_iter().for_each(|event| match event {
        //     EntityStepResult::ChunkTicketRemoved(ticket_key) => {
        //         self.chunk_tickets.remove(&ticket_key).unwrap();
        //     }
        //     EntityStepResult::ChunkTicketMoved(ticket_key, offset) => {
        //         let mut ticket = self.chunk_tickets.remove(&ticket_key).unwrap();
        //         ticket.translate(offset);
        //         self.chunk_tickets.insert(ticket_key, ticket);
        //     }
        // });
    }

    pub fn unsafe_get(&self, coords: &WorldRegionCoords) -> Option<&Region> {
        self.regions.get(coords)
    }

    pub fn unsafe_get_mut(&mut self, coords: &WorldRegionCoords) -> Option<&mut Region> {
        self.regions.get_mut(coords)
    }
}

pub enum EntityStepResult {
    ChunkTicketMoved(ChunkTicketKey, (i32, i32)),
    ChunkTicketRemoved(ChunkTicketKey),
}
