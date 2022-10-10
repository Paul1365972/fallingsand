#![feature(test)]

extern crate test;

use fallingsand_sim::{
    cell::cell::SimulationCell,
    chunk::EntityChunk,
    coords::ChunkCoords,
    myimpl::{tile::MyTile, tilesimulator::Context},
    region::{Chunk, DisjointRegion},
};
#[cfg(test)]
use fallingsand_sim::{chunk::TileChunk, coords::WorldChunkCoords};
use test::Bencher;

fn create_filled_field() -> DisjointRegion<MyTile, ()> {
    let mut field = DisjointRegion::new_unchecked();
    for y in 0..30 {
        for x in 0..30 {
            field.insert(
                WorldChunkCoords::new(x, y),
                Chunk::new(TileChunk::new_air_sand_mix(), EntityChunk::default()),
            );
        }
    }
    field
}

#[bench]
fn allocate_norm_chunks(b: &mut Bencher) {
    b.iter(|| {
        for _ in 0..30 {
            for _ in 0..30 {
                TileChunk::new_air_sand_mix();
            }
        }
    });
}

#[bench]
fn insert_norm_chunks(b: &mut Bencher) {
    b.iter(|| {
        create_filled_field();
    });
}

#[bench]
fn remove_insert_norm_chunks(b: &mut Bencher) {
    let mut field = create_filled_field();
    b.iter(|| {
        for y in 0..30 {
            for x in 0..30 {
                let coords = WorldChunkCoords::new(x, y);
                let chunk = field.remove(coords).unwrap();
                field.insert(coords, chunk);
            }
        }
    });
}

#[bench]
fn clone_norm_chunks(b: &mut Bencher) {
    let mut field = create_filled_field();
    let mut blackhole = 0;
    println!(
        "Tile Chunk size: {}",
        std::mem::size_of::<TileChunk<MyTile>>()
    );
    println!("Tile size: {}", std::mem::size_of::<MyTile>());
    println!(
        "WorldChunkCoords size: {}",
        std::mem::size_of::<WorldChunkCoords>()
    );
    println!(
        "Region size: {}",
        std::mem::size_of::<DisjointRegion<MyTile, ()>>()
    );
    b.iter(|| {
        for y in 0..30 {
            for x in 0..30 {
                let coords = WorldChunkCoords::new(x, y);
                let chunk = field.get(coords).unwrap();
                let clone = chunk.clone();
                blackhole += clone.tile_chunk().get(ChunkCoords::new(0, 0)).temperature;
            }
        }
    });
}

#[bench]
fn step_empty_chunks(b: &mut Bencher) {
    let mut field = DisjointRegion::<MyTile, ()>::new_unchecked();
    for y in 0..30 {
        for x in 0..30 {
            field.insert(
                WorldChunkCoords::new(x, y),
                Chunk::new(TileChunk::new_air(), EntityChunk::default()),
            );
        }
    }
    let mut ctx = Context::default();
    b.iter(|| {
        field.step_tiles(|x: &mut SimulationCell<MyTile>| x.step(&ctx));
        ctx.tick += 1;
    });
}

#[bench]
fn step_filled_chunks(b: &mut Bencher) {
    let mut field = create_filled_field();
    let mut ctx = Context::default();
    b.iter(|| {
        field.step_tiles(|x: &mut SimulationCell<MyTile>| x.step(&ctx));
        ctx.tick += 1;
    });
}

#[bench]
fn build_active_chunk_list(b: &mut Bencher) {
    let mut field = DisjointRegion::<MyTile, ()>::new_unchecked();
    for y in 0..30 {
        for x in 0..30 {
            field.insert(
                WorldChunkCoords::new(x, y),
                Chunk::new(TileChunk::new_air_sand_mix(), EntityChunk::default()),
            );
        }
    }
    b.iter(|| {
        field.build_active_chunks();
    });
}
