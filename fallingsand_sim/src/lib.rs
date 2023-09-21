#![feature(map_many_mut)]

use cell::tile::{MyTile, MyTileVariant};
use chunk::{TileChunk, UnloadedRegion};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};
use util::{coords::WorldRegionCoords, timer::StepTimer};
use world::World;

pub mod cell;
pub mod chunk;
pub mod chunk_tickets;
pub mod entity;
pub mod orchestrator;
pub mod util;
pub mod world;
pub mod region_manager;

pub struct Server {
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn run(&mut self) {
        self.running.store(true, Ordering::Relaxed);
        let running = self.running.clone();
        self.handle = Some(
            std::thread::Builder::new()
                .name("Main Simulation Thread".into())
                .stack_size(8 * 1024 * 1024)
                .spawn(move || {
                    println!("Initalizing World...");
                    //let mut world = World::default();
                    let mut world = create_world();
                    let mut timer = StepTimer::new(Duration::from_secs_f64(1.0 / 60.0));
                    while running.load(Ordering::Relaxed) {
                        world.step(vec![]);
                        timer.sleep();
                    }
                })
                .unwrap(),
        );
    }

    pub fn stop(self) {
        self.running.store(false, Ordering::Relaxed);
        self.handle.unwrap().join().unwrap();
    }
}

fn create_world() -> World {
    let mut world = World::default();
    for y in -6..=6 {
        for x in -6..=6 {
            let chunks = std::array::from_fn(|_| {
                TileChunk::new(std::array::from_fn(|_| MyTile {
                    variant: MyTileVariant::AIR,
                    ..Default::default()
                }))
            });
            world.load_region(
                WorldRegionCoords::new(x, y),
                UnloadedRegion {
                    tile_chunk: Box::new(chunks),
                    entities: vec![],
                },
            );
        }
    }
    world
}
