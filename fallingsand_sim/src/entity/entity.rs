use serde::{Deserialize, Serialize};

use crate::chunk_tickets::ChunkTicketKey;

#[derive(Serialize, Deserialize)]
pub struct Entity {
    pub location: (i16, i16),
    pub velocity: (i16, i16),
    pub variant: EntityVariant,
}

#[derive(Serialize, Deserialize)]
pub enum EntityVariant {
    Player {
        #[serde(skip)]
        chunk_ticket_key: Option<ChunkTicketKey>,
    },
}

impl Entity {
    pub fn step(&mut self) {
        todo!()
    }

    pub fn apply_move(&mut self) -> (i32, i32) {
        todo!()
    }

    pub fn should_remove(&mut self) -> bool {
        todo!()
    }
}
