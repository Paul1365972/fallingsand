mod network;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use fallingsand_sim::cell::tile::{Tile, TileVariant};
use fallingsand_sim::chunk::{TileChunk, UnloadedRegion};
use fallingsand_sim::network::ClientMap;
use fallingsand_sim::util::coords::WorldRegionCoords;
use fallingsand_sim::util::timer::StepTimer;
use fallingsand_sim::world::World;
use network::NetworkManager;
use rustyline::error::ReadlineError;
use rustyline::{DefaultEditor, Result};

fn main() -> Result<()> {
    println!("Starting server!");

    let running = Arc::new(AtomicBool::new(true));

    let inner_running = running.clone();
    let handle = std::thread::Builder::new()
        .name("Main Simulation Thread".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let mut client_map = ClientMap::default();
            let mut network_manager = NetworkManager::new("127.0.0.1:8080");

            println!("Initalizing World...");
            // let mut world = World::default();
            let mut world = create_world();
            let mut timer = StepTimer::new(Duration::from_secs_f64(1.0 / 60.0));
            while inner_running.load(Ordering::Relaxed) {
                world.step(&mut client_map);
                network_manager.tick(&mut client_map);
                timer.sleep();
            }
            network_manager.close();
        })
        .unwrap();

    let mut rl = DefaultEditor::new()?;
    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => match line.to_ascii_lowercase().as_str() {
                "exit" => {
                    break;
                }
                _ => {
                    println!("Invalid command: {}", line);
                }
            },
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    running.store(false, Ordering::Relaxed);
    handle.join().unwrap();
    Ok(())
}

fn create_world() -> World {
    let mut world = World::default();
    for y in -6..=6 {
        for x in -6..=6 {
            let chunks = std::array::from_fn(|_| {
                TileChunk::new(std::array::from_fn(|_| Tile {
                    variant: TileVariant::AIR,
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
