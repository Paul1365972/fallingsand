use rustc_hash::{FxHashMap, FxHashSet};

use crate::util::coords::WorldRegionCoords;

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct ChunkTicketKey(u32);

pub struct ChunkTicket {
    key: ChunkTicketKey,
    coords: WorldRegionCoords,
    level: u8,
}

#[derive(Default)]
pub struct ChunkTicketField {
    tickets: FxHashMap<ChunkTicketKey, (WorldRegionCoords, u8)>,
}

impl ChunkTicketField {
    pub fn add_ticket(&mut self, ticket: ChunkTicket) {
        let old = self
            .tickets
            .insert(ticket.key, (ticket.coords, ticket.level));
        assert!(old.is_none());
    }

    pub fn remove_ticket(&mut self, key: ChunkTicketKey) {
        self.tickets.remove(&key).unwrap();
    }

    pub fn insert_active_chunks(&self, set: &mut FxHashSet<WorldRegionCoords>) {
        for (coords, level) in self.tickets.values() {
            let level = *level as u32 as i32;
            for dy in level..=level {
                for dx in level..=level {
                    set.insert(coords + (dx, dy));
                }
            }
        }
    }

    pub fn insert_border_chunks(&self, distance: i32, set: &mut FxHashSet<WorldRegionCoords>) {
        for (coords, level) in self.tickets.values() {
            let level = (*level as i32).saturating_add(distance);
            set.insert(coords + (level, level));
            set.insert(coords + (level, -level));
            set.insert(coords + (-level, level));
            set.insert(coords + (-level, -level));
            for d in (1 - level)..=(level - 1) {
                set.insert(coords + (d, level));
                set.insert(coords + (d, -level));
                set.insert(coords + (level, d));
                set.insert(coords + (-level, d));
            }
        }
    }
}
