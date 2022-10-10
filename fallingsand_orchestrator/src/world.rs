use std::num::NonZeroU16;

use crate::{
    cell::{
        cell::TileTransitionFn,
        simulator::{EntityStepEvent, EntityStepResult},
    },
    coords::{ChunkCoords, WorldChunkCoords},
    region::DisjointRegion, chunk_ticket::{ChunkTicketKey, ChunkTicket},
};
use itertools::Itertools;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

pub struct World<T, E> {
    regions: Vec<DisjointRegion<T, E>>,
    chunk_tickets: FxHashMap<ChunkTicketKey, ChunkTicket>,
}



enum ExternalEvents {
    LOAD_CHUNK(ChunkCoords),
}

pub struct PlayerInputState {
    id: u32, // ???
    pressed_keys: u16,
}

impl<T: Send, E: Entity> World<T, E> {
    fn load_chunks(&mut self, coords: &[WorldChunkCoords]) {
        for coord in coords {
            let region = self.get_region(coord);
        }
        for region in self.regions {
            for region in self.regions {}
        }
    }

    pub fn get_region(&self, coords: &WorldChunkCoords) -> Option<&DisjointRegion<T, E>> {
        let regions = self
            .regions
            .iter()
            .filter(|r| r.contains_chunk(coords))
            .collect_vec();
        assert!(
            regions.len() <= 1,
            "Multiple regions contain the same chunk, we fked up"
        );
        regions.into_iter().next()
    }

    pub fn step<F>(
        &mut self,
        tile_transition_fn: F,
        players: Vec<PlayerInputState>,
    ) where
        F: TileTransitionFn<T>,
    {
        // pre: already received network packets
        // send network packets
        self.regions.par_iter_mut().for_each(|region| {
            region.step_tiles(tile_transition_fn.clone());
            // apply queued region tile events
        });
        // apply queued global tile events
        
        
        // Step entities
        let mut entity_step_results = Vec::new();
        self.regions
            .par_iter_mut()
            .map(|region| region.step_entities())
            .collect_into_vec(&mut entity_step_results);

        // Execute entity step events
        entity_step_results
            .iter()
            .flat_map(|x| x.events())
            .for_each(|event| {
                match event {
                    EntityStepEvent::TicketEntityMoved(key, offset) => {
                        let ticket = self.chunk_tickets.get_mut(key).unwrap();
                        ticket.translate(*offset);
                    }
                    EntityStepEvent::TicketEntityRemoved(key) => {
                        self.chunk_tickets.remove(key).unwrap();
                    }
                };
            })
    }
}
