#![feature(map_many_mut)]

use std::time::Duration;

use bevy::prelude::*;

pub mod cell;
pub mod chunk;
//pub mod chunk_cache;
pub mod chunk_tickets;
pub mod entity;
pub mod orchestrator;
pub mod util;
pub mod world;

struct Server {
    app: App,
}

struct ServerExecutor {
    server: Server,
}

// impl Server {
//     fn new(hosted: bool) -> Self {
//         let mut app = App::new();
//         app.insert_resource(ScheduleRunnerSettings::run_loop(Duration::from_millis(2))); // TODO
//         app.run();
//         app.insert_sub_app(label, sub_app)
//         Server { app: () }
//     }
//
//     fn tick(&mut self) {
//         self.app.update();
//     }
// }
//
// #[derive(Resource, Default, Deref, DerefMut)]
// pub struct ChunkMap(HashMap<ChunkPos, Entity, FxBuildHasher>);
//
// #[derive(SystemParam)]
// pub struct ChunkMapper<'w, 's> {
//     map: Res<'w, ChunkMap>,
//     chunks: Query<'w, 's, &'static Chunk, With<IsChunk>>,
// }
//
// impl<'w, 's> ChunkMapper<'w, 's> {
//     pub(crate) fn get_chunk(&self, position: &ChunkPos) -> Option<&Chunk> {
//         let entity = self.map.get(position)?;
//         self.chunks.get(*entity).ok()
//     }
// }
//
