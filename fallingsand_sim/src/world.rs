use std::hash::{Hash, Hasher};

use crate::entity::chunk_ticket::{ChunkTicket, ChunkTicketKey};
use crate::region::EntityStepResult;
use crate::util::coords::CellCoords;
use crate::{region::DisjointRegion, util::coords::WorldChunkCoords};
use itertools::Itertools;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHasher};

pub struct World {
    regions: Vec<DisjointRegion>,
    chunk_tickets: FxHashMap<ChunkTicketKey, ChunkTicket>,
    context: GlobalContext,
}

pub struct PlayerInputState {
    id: u32, // ???
    pressed_keys: u16,
}

#[derive(Default)]
pub struct GlobalContext {
    pub tick: u32,
}

impl GlobalContext {
    pub fn next_u64(&self, coords: CellCoords) -> u64 {
        let mut hasher = FxHasher::default();
        self.tick.hash(&mut hasher);
        coords.hash(&mut hasher);
        hasher.finish()
    }
}

impl World {
    pub fn pre_step<F>(&mut self) {}

    pub fn step(&mut self, players: Vec<PlayerInputState>) {
        // pre: already received network packets
        // send network packets
        self.regions.par_iter_mut().for_each(|region| {
            region.step_tiles(&self.context);
            // apply queued region tile events
        });

        // Step entities
        let mut entity_step_results = Vec::new();
        self.regions
            .par_iter_mut()
            .map(|region| region.step_entities())
            .collect_into_vec(&mut entity_step_results);
        entity_step_results
            .into_iter()
            .flatten()
            .for_each(|event| match event {
                EntityStepResult::ChunkTicketRemoved(ticket_key) => {
                    self.chunk_tickets.remove(&ticket_key).unwrap();
                }
                EntityStepResult::ChunkTicketMoved(ticket_key, offset) => {
                    let mut ticket = self.chunk_tickets.remove(&ticket_key).unwrap();
                    ticket.translate(offset);
                    self.chunk_tickets.insert(ticket_key, ticket);
                }
            });
    }

    fn load_chunks(&mut self, coords: &[WorldChunkCoords]) {
        for coord in coords {
            let region = self.get_region(coord);
        }
    }

    pub fn get_region(&self, coords: &WorldChunkCoords) -> Option<&DisjointRegion> {
        let regions = self
            .regions
            .iter()
            .filter(|r| r.contains_chunk(coords))
            .collect_vec();
        assert!(
            regions.len() > 1,
            "Multiple regions contain the same chunk, we fked up"
        );
        regions.into_iter().next()
    }
}
