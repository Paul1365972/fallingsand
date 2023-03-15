#![feature(test)]
#[cfg(test)]
extern crate test;

use fallingsand_sim::{
    cell::{
        tile::{MyTile, MyTileVariant},
    },
    region::DisjointRegion,
    util::coords::{ChunkCoords, TILES_PER_CHUNK},
    world::GlobalContext,
};
use fallingsand_sim::{chunk::TileChunk, util::coords::WorldChunkCoords};
use test::Bencher;

fn create_filled_field() -> DisjointRegion {
    let mut field = DisjointRegion::new_unchecked();
    for y in 0..30 {
        for x in 0..30 {
            field.insert_tile_chunk(WorldChunkCoords::new(x, y), new_air_sand_chunk());
        }
    }
    field
}

pub fn new_air_chunk() -> TileChunk {
    TileChunk::new(
        [MyTile {
            variant: MyTileVariant::AIR,
            ..Default::default()
        }; TILES_PER_CHUNK as usize * TILES_PER_CHUNK as usize],
    )
}

pub fn new_air_sand_chunk() -> TileChunk {
    let mut chunk = TileChunk::new(
        [MyTile {
            variant: MyTileVariant::AIR,
            ..Default::default()
        }; TILES_PER_CHUNK as usize * TILES_PER_CHUNK as usize],
    );
    for y in 0..TILES_PER_CHUNK {
        for x in 0..TILES_PER_CHUNK {
            if (x + y) & 4 == 0 {
                chunk.get_mut(ChunkCoords::new(x, y)).variant = MyTileVariant::SAND;
            }
        }
    }
    chunk
}

#[bench]
fn allocate_norm_chunks(b: &mut Bencher) {
    b.iter(|| {
        for _ in 0..30 {
            for _ in 0..30 {
                new_air_sand_chunk();
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
                let chunk = field.unsafe_remove_tile_chunk(coords).unwrap();
                field.insert_tile_chunk(coords, chunk);
            }
        }
    });
}

#[bench]
fn clone_norm_chunks(b: &mut Bencher) {
    let mut field = create_filled_field();
    let mut blackhole = 0;
    println!("Tile Chunk size: {}", std::mem::size_of::<TileChunk>());
    println!("Tile size: {}", std::mem::size_of::<MyTile>());
    println!(
        "WorldChunkCoords size: {}",
        std::mem::size_of::<WorldChunkCoords>()
    );
    println!("Region size: {}", std::mem::size_of::<DisjointRegion>());
    b.iter(|| {
        for y in 0..30 {
            for x in 0..30 {
                let coords = WorldChunkCoords::new(x, y);
                let chunk = field.unsafe_get(coords).unwrap();
                let clone = chunk.clone();
                blackhole += clone.tile_chunk().get(ChunkCoords::new(0, 0)).temperature;
            }
        }
    });
}

#[bench]
fn step_empty_chunks(b: &mut Bencher) {
    let mut field = DisjointRegion::new_unchecked();
    for y in 0..30 {
        for x in 0..30 {
            field.insert_tile_chunk(WorldChunkCoords::new(x, y), new_air_chunk());
        }
    }
    let mut ctx = GlobalContext::default();
    b.iter(|| {
        field.step_tiles(&ctx);
        ctx.tick += 1;
    });
}

#[bench]
fn step_filled_chunks(b: &mut Bencher) {
    let mut field = create_filled_field();
    let mut ctx = GlobalContext::default();
    b.iter(|| {
        field.step_tiles(&ctx);
        ctx.tick += 1;
    });
}

#[bench]
fn build_active_chunk_list(b: &mut Bencher) {
    let mut field = DisjointRegion::new_unchecked();
    for y in 0..30 {
        for x in 0..30 {
            field.insert_tile_chunk(WorldChunkCoords::new(x, y), new_air_sand_chunk());
        }
    }
    b.iter(|| {
        field.build_active_chunks();
    });
}

#[bench]
fn step_no_tiles(b: &mut Bencher) {
    let mut field = DisjointRegion::new_unchecked();
    let mut ctx = GlobalContext::default();
    b.iter(|| {
        field.step_tiles(&ctx);
        ctx.tick += 1;
    });
}

#[bench]
fn step_no_entities(b: &mut Bencher) {
    let mut field = DisjointRegion::new_unchecked();
    b.iter(|| {
        field.step_entities();
    });
}
