use crate::obstacles::Obstacles;
use crate::rules;
use crate::window::{SimWindow, WINDOW_CHUNKS, WINDOW_SLOTS, spill};
use crate::world::CellWorld;
use fallingsand_core::{CHUNK_SIZE, CellPos, Chunk, ChunkPos, DirtyRect, MaterialRegistry};
use rustc_hash::FxHashSet;

pub fn step(world: &mut CellWorld, registry: &MaterialRegistry, obstacles: &Obstacles) {
    world.advance_tick();
    let tick = world.tick();
    for chunk in world.chunk_map_mut().values_mut() {
        chunk.swap_bounds();
        if chunk.old_bounds.is_empty() {
            chunk.sleeping = true;
        }
    }
    for phase in 0..4 {
        run_phase(world, registry, obstacles, phase, tick);
    }
    world.apply_edits();
}

fn run_phase(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    obstacles: &Obstacles,
    phase: u32,
    tick: u64,
) {
    let px = (phase & 1) as i32;
    let py = ((phase >> 1) & 1) as i32;

    let map = world.chunk_map_mut();
    let mut blocks: FxHashSet<(i32, i32)> = FxHashSet::default();
    for (&pos, chunk) in map.iter() {
        if chunk.dirty().is_empty() {
            continue;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                let neighbor = pos.translated(dx, dy);
                let block = (neighbor.x >> 1, neighbor.y >> 1);
                if (block.0 & 1) == px && (block.1 & 1) == py {
                    blocks.insert(block);
                }
            }
        }
    }

    let mut windows = Vec::with_capacity(blocks.len());
    for &(bx, by) in &blocks {
        let origin = ChunkPos::new((bx << 1) - 1, (by << 1) - 1);
        let mut slots: [Option<Chunk>; WINDOW_SLOTS] = std::array::from_fn(|_| None);
        for sy in 0..WINDOW_CHUNKS {
            for sx in 0..WINDOW_CHUNKS {
                slots[(sy * WINDOW_CHUNKS + sx) as usize] = map.remove(&origin.translated(sx, sy));
            }
        }
        windows.push(SimWindow::new(origin, slots, tick));
    }

    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        windows
            .par_iter_mut()
            .for_each(|window| process_block(window, registry, obstacles, tick));
    }
    #[cfg(not(feature = "parallel"))]
    for window in windows.iter_mut() {
        process_block(window, registry, obstacles, tick);
    }

    for window in windows {
        let (origin, slots) = window.into_parts();
        for (index, slot) in slots.into_iter().enumerate() {
            if let Some(chunk) = slot {
                let sx = index as i32 % WINDOW_CHUNKS;
                let sy = index as i32 / WINDOW_CHUNKS;
                map.insert(origin.translated(sx, sy), chunk);
            }
        }
    }
}

fn process_block(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    obstacles: &Obstacles,
    tick: u64,
) {
    let mut rects = [[DirtyRect::EMPTY; 2]; 2];
    for oy in 0..2i32 {
        for ox in 0..2i32 {
            let (sx, sy) = (ox + 1, oy + 1);
            if window.chunk_at(sx, sy).is_none() {
                continue;
            }
            let mut rect = DirtyRect::EMPTY;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(neighbor) = window.chunk_at(sx + dx, sy + dy) {
                        rect = rect.union(spill(neighbor.dirty(), dx, dy));
                    }
                }
            }
            rects[oy as usize][ox as usize] = rect;
            if !rect.is_empty() {
                window.wake_chunk(sx, sy);
            }
        }
    }

    let tick_byte = tick as u8;
    let size = CHUNK_SIZE as i32;
    let origin_cell = window.origin().translated(1, 1).base_cell();
    for gy in 0..2 * size {
        let oy = (gy / size) as usize;
        let ly = (gy % size) as u8;
        let reverse = (tick as i32 + gy) & 1 == 1;
        for ox_index in 0..2usize {
            let ox = if reverse { 1 - ox_index } else { ox_index };
            let rect = rects[oy][ox];
            if rect.is_empty() || ly < rect.min_y || ly > rect.max_y {
                continue;
            }
            let (start, end) = (rect.min_x as i32, rect.max_x as i32);
            for i in 0..=(end - start) {
                let lx = if reverse { end - i } else { start + i };
                let pos = CellPos::new(origin_cell.x + ox as i32 * size + lx, origin_cell.y + gy);
                rules::update_cell(window, registry, obstacles, pos, tick, tick_byte);
            }
        }
    }
}
