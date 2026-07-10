use crate::rules;
use crate::window::{SimWindow, WINDOW_CHUNKS, WINDOW_SLOTS};
use crate::world::CellWorld;
use fallingsand_core::{CHUNK_SIZE, CellPos, Chunk, ChunkPos, DirtyRect, MaterialRegistry};
use rustc_hash::FxHashSet;

pub fn step_scoped(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    simulate: &(dyn Fn(ChunkPos) -> bool + Sync),
) {
    world.advance_tick();
    let tick = world.tick();
    let loaded: FxHashSet<ChunkPos> = world.chunk_map_mut().keys().copied().collect();
    for (&pos, chunk) in world.chunk_map_mut().iter_mut() {
        let ready = simulate(pos)
            && (-1..=1).all(|dy| (-1..=1).all(|dx| loaded.contains(&pos.translated(dx, dy))));
        if !ready {
            chunk.sleeping = true;
            continue;
        }
        chunk.swap_rects();
        if chunk.prev_sim.is_empty() {
            chunk.sleeping = true;
        }
    }
    for phase in 0..4 {
        run_phase(world, registry, phase, tick, simulate);
    }
}

fn run_phase(
    world: &mut CellWorld,
    registry: &MaterialRegistry,
    phase: u32,
    tick: u64,
    simulate: &(dyn Fn(ChunkPos) -> bool + Sync),
) {
    let px = (phase & 1) as i32;
    let py = ((phase >> 1) & 1) as i32;

    let map = world.chunk_map_mut();
    let mut blocks: FxHashSet<(i32, i32)> = FxHashSet::default();
    for (&pos, chunk) in map.iter() {
        if chunk.sim_rect().is_empty() || !simulate(pos) {
            continue;
        }
        let block = (pos.x >> 1, pos.y >> 1);
        if (block.0 & 1) == px && (block.1 & 1) == py {
            blocks.insert(block);
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
            .for_each(|window| process_block(window, registry, tick, simulate));
    }
    #[cfg(not(feature = "parallel"))]
    for window in windows.iter_mut() {
        process_block(window, registry, tick, simulate);
    }

    let mut structural: Vec<CellPos> = Vec::new();
    let mut damage: Vec<CellPos> = Vec::new();
    for window in windows {
        let parts = window.into_parts();
        structural.extend(parts.structural);
        damage.extend(parts.damage);
        for (index, slot) in parts.slots.into_iter().enumerate() {
            if let Some(chunk) = slot {
                let sx = index as i32 % WINDOW_CHUNKS;
                let sy = index as i32 / WINDOW_CHUNKS;
                map.insert(parts.origin.translated(sx, sy), chunk);
            }
        }
    }
    world.push_structural(structural);
    world.push_damage(damage);
}

fn process_block(
    window: &mut SimWindow,
    registry: &MaterialRegistry,
    tick: u64,
    simulate: &(dyn Fn(ChunkPos) -> bool + Sync),
) {
    let mut rects = [[DirtyRect::EMPTY; 2]; 2];
    for oy in 0..2i32 {
        for ox in 0..2i32 {
            let (sx, sy) = (ox + 1, oy + 1);
            if !simulate(window.origin().translated(sx, sy))
                || (-1..=1).any(|dy| (-1..=1).any(|dx| window.chunk_at(sx + dx, sy + dy).is_none()))
            {
                continue;
            }
            let rect = window
                .chunk_at(sx, sy)
                .map_or(DirtyRect::EMPTY, |chunk| chunk.sim_rect());
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
                rules::update_cell(window, registry, pos, tick, tick_byte);
            }
        }
    }
}
