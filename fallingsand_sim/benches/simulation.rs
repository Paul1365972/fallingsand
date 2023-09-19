#![feature(test)]
#[cfg(test)]
extern crate test;

use fallingsand_sim::{
    cell::tile::{MyTile, MyTileVariant},
    chunk::{TileChunk, UnloadedRegion},
    util::coords::{WorldRegionCoords, CHUNKS_PER_REGION, TILES_PER_CHUNK},
    world::World,
};
use test::Bencher;

fn empty_world() -> World {
    let mut world = World::default();
    for y in -1..=1 {
        for x in -1..=1 {
            let chunks = std::array::from_fn(|_| {
                TileChunk::new(std::array::from_fn(|_| MyTile {
                    variant: MyTileVariant::AIR,
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

fn filled_world() -> World {
    let mut world = empty_world();
    let region = world.unsafe_get_mut(&WorldRegionCoords::new(0, 0)).unwrap();
    for i in 0..(CHUNKS_PER_REGION * CHUNKS_PER_REGION) {
        for j in 0..(TILES_PER_CHUNK * TILES_PER_CHUNK) {
            if (j + (j / TILES_PER_CHUNK) % 4) == 0 {
                region.chunks[i].tiles[j].variant =
                    MyTileVariant::STONE;
            }
        }
    }
    world
}

#[bench]
fn step_empty_chunks(b: &mut Bencher) {
    let mut world = empty_world();
    b.iter(|| {
        world.step_context();
        world.step_tiles();
    });
}

#[bench]
fn step_filled_chunks(b: &mut Bencher) {
    let mut world = filled_world();
    b.iter(|| {
        world.step_context();
        world.step_tiles();
    });
}

#[bench]
fn step_no_entities(b: &mut Bencher) {
    let mut world = empty_world();
    b.iter(|| {
        world.step_entities();
    });
}
