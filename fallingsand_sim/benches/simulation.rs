#![feature(test)]
#[cfg(test)]
extern crate test;

use std::{mem::ManuallyDrop, hint::black_box};

use fallingsand_sim::{
    cell::tile::{MyTile, MyTileVariant},
    chunk::{TileChunk, UnloadedRegion},
    util::coords::{WorldRegionCoords, CHUNKS_PER_REGION, TILES_PER_CHUNK},
    world::World,
};
use test::Bencher;

fn empty_world() -> World {
    let mut world = World::default();
    for y in -2..=2 {
        for x in -2..=2 {
            let mut chunks = Vec::with_capacity(CHUNKS_PER_REGION * CHUNKS_PER_REGION);
            for _ in 0..(CHUNKS_PER_REGION * CHUNKS_PER_REGION) {
                chunks.push(TileChunk::new(std::array::from_fn(|_| MyTile {
                    variant: MyTileVariant::AIR,
                    ..Default::default()
                })));
            }
            let chunks = unsafe {
                Box::from_raw(ManuallyDrop::new(chunks).as_mut_ptr() as *mut [TileChunk; CHUNKS_PER_REGION * CHUNKS_PER_REGION])
            };
            world.load_region(
                WorldRegionCoords::new(x, y),
                UnloadedRegion {
                    tile_chunk: chunks,
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
                region.chunks[i].tiles[j].variant = MyTileVariant::SAND;
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

#[bench]
fn create_world(b: &mut Bencher) {
    b.iter(|| {
        black_box(empty_world())
    });
}
