use crate::rules;
use crate::window::{SimWindow, WINDOW_CHUNKS};
use crate::world::CellWorld;
use fallingsand_core::{CHUNK_SIZE, CellPos, Chunk, ChunkPos, DirtyRect};
use fallingsand_math::Hash;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeSet;
use std::time::Instant;

const _: () = assert!(WINDOW_CHUNKS == 4);
const RANDOM_TICKS_PER_CHUNK: u32 = 4;
const RANDOM_TICK_SAMPLE_SALT: Hash = Hash::label("simulation.random_tick_sample");
const PHASE_ORDER_SALT: Hash = Hash::label("simulation.phase_order");
const ROW_DIRECTION_SALT: Hash = Hash::label("simulation.row_direction");

const PHASE_ORDERS: [[u32; 4]; 24] = [
    [0, 1, 2, 3],
    [0, 1, 3, 2],
    [0, 2, 1, 3],
    [0, 2, 3, 1],
    [0, 3, 1, 2],
    [0, 3, 2, 1],
    [1, 0, 2, 3],
    [1, 0, 3, 2],
    [1, 2, 0, 3],
    [1, 2, 3, 0],
    [1, 3, 0, 2],
    [1, 3, 2, 0],
    [2, 0, 1, 3],
    [2, 0, 3, 1],
    [2, 1, 0, 3],
    [2, 1, 3, 0],
    [2, 3, 0, 1],
    [2, 3, 1, 0],
    [3, 0, 1, 2],
    [3, 0, 2, 1],
    [3, 1, 0, 2],
    [3, 1, 2, 0],
    [3, 2, 0, 1],
    [3, 2, 1, 0],
];

type Simulate<'a> = dyn Fn(ChunkPos) -> bool + Sync + 'a;
type Schedule<'a> = dyn Fn(ChunkPos, &Chunk) -> bool + 'a;
type Kernel<'a> = dyn Fn(&mut SimWindow) + Sync + 'a;

#[derive(Debug, Default, Clone, Copy)]
pub struct SimTimings {
    pub simulate_micros: u32,
    pub random_tick_micros: u32,
}

pub fn step_scoped(
    world: &mut CellWorld,
    simulate: &Simulate,
    random_tick: &Simulate,
) -> SimTimings {
    world.advance_tick();
    let tick = world.tick();

    let ready: FxHashSet<ChunkPos> = world
        .chunks()
        .filter(|&(pos, _)| {
            simulate(pos)
                && (-1..=1)
                    .all(|dy| (-1..=1).all(|dx| world.chunk(pos.translated(dx, dy)).is_some()))
        })
        .map(|(pos, _)| pos)
        .collect();
    world
        .chunk_map_mut()
        .par_iter_mut()
        .for_each(|(pos, chunk)| chunk.begin_tick(ready.contains(pos)));

    let simulate_micros = run_sim(
        "simulate",
        world,
        tick,
        &|pos, chunk| simulate(pos) && !chunk.sim_rect().is_empty(),
        &|window| simulate_block(window, tick, simulate),
    );

    let random_tick_micros = run_sim(
        "random_tick",
        world,
        tick,
        &|pos, _| random_tick(pos),
        &|window| random_tick_block(window, tick, random_tick),
    );

    SimTimings {
        simulate_micros,
        random_tick_micros,
    }
}

fn run_sim(
    name: &'static str,
    world: &mut CellWorld,
    tick: u64,
    schedule: &Schedule,
    kernel: &Kernel,
) -> u32 {
    let _span = tracing::info_span!("sim", name).entered();
    let start = Instant::now();
    let order = Hash::seed(tick)
        .salt(PHASE_ORDER_SALT)
        .choose(&PHASE_ORDERS);
    for phase in order {
        run_phase(world, phase, schedule, kernel);
    }
    start.elapsed().as_micros() as u32
}

fn run_phase(world: &mut CellWorld, phase: u32, schedule: &Schedule, kernel: &Kernel) {
    let _span = tracing::info_span!("sim_phase", phase).entered();
    let px = (phase & 1) as i32;
    let py = ((phase >> 1) & 1) as i32;

    let map = world.chunk_map_mut();
    let origins: BTreeSet<ChunkPos> = map
        .iter()
        .filter(|(pos, chunk)| {
            let block = (pos.x >> 1, pos.y >> 1);
            (block.0 & 1) == px && (block.1 & 1) == py && schedule(**pos, chunk)
        })
        .map(|(pos, _)| ChunkPos::new(((pos.x >> 1) << 1) - 1, ((pos.y >> 1) << 1) - 1))
        .collect();

    let mut members: FxHashMap<ChunkPos, (usize, i32, i32)> = FxHashMap::default();
    for (index, &origin) in origins.iter().enumerate() {
        for sy in 0..WINDOW_CHUNKS {
            for sx in 0..WINDOW_CHUNKS {
                members.insert(origin.translated(sx, sy), (index, sx, sy));
            }
        }
    }

    let mut windows: Vec<SimWindow> = origins
        .iter()
        .map(|&origin| SimWindow::new(origin, std::array::from_fn(|_| None)))
        .collect();
    for (&pos, chunk) in map.iter_mut() {
        if let Some(&(index, sx, sy)) = members.get(&pos) {
            windows[index].set_slot(sx, sy, chunk);
        }
    }

    windows.par_iter_mut().for_each(kernel);

    let mut structural: Vec<CellPos> = Vec::new();
    let mut damage: Vec<CellPos> = Vec::new();
    for window in windows {
        let parts = window.into_parts();
        structural.extend(parts.structural);
        damage.extend(parts.damage);
    }
    world.push_structural(structural);
    world.push_damage(damage);
}

fn owned_chunks(window: &SimWindow, simulate: &Simulate) -> [[bool; 2]; 2] {
    let mut owned = [[false; 2]; 2];
    for (oy, row) in owned.iter_mut().enumerate() {
        for (ox, slot) in row.iter_mut().enumerate() {
            let (sx, sy) = (ox as i32 + 1, oy as i32 + 1);
            *slot = simulate(window.origin().translated(sx, sy))
                && (-1..=1)
                    .all(|dy| (-1..=1).all(|dx| window.chunk_at(sx + dx, sy + dy).is_some()));
        }
    }
    owned
}

fn simulate_block(window: &mut SimWindow, tick: u64, simulate: &Simulate) {
    let owned = owned_chunks(window, simulate);
    let mut rects = [[DirtyRect::EMPTY; 2]; 2];
    for (oy, row) in owned.iter().enumerate() {
        for (ox, &is_owned) in row.iter().enumerate() {
            if is_owned {
                rects[oy][ox] = window
                    .chunk_at(ox as i32 + 1, oy as i32 + 1)
                    .map_or(DirtyRect::EMPTY, |chunk| chunk.sim_rect());
            }
        }
    }

    let size = CHUNK_SIZE as i32;
    let origin_cell = window.origin().translated(1, 1).base_cell();
    for gy in 0..2 * size {
        let oy = (gy / size) as usize;
        let ly = (gy % size) as u8;
        let reverse = Hash::seed(tick)
            .salt(ROW_DIRECTION_SALT)
            .pos(0, origin_cell.y + gy)
            .bit();
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
                rules::update_cell(window, pos, tick);
            }
        }
    }
}

fn random_tick_block(window: &mut SimWindow, tick: u64, simulate: &Simulate) {
    let owned = owned_chunks(window, simulate);
    let size = CHUNK_SIZE as i32;
    for (oy, row) in owned.iter().enumerate() {
        for (ox, &is_owned) in row.iter().enumerate() {
            if !is_owned {
                continue;
            }
            let cp = window.origin().translated(ox as i32 + 1, oy as i32 + 1);
            let base = cp.base_cell();
            let mut rng = Hash::seed(tick)
                .salt(RANDOM_TICK_SAMPLE_SALT)
                .pos(cp.x, cp.y)
                .rng();
            for _ in 0..RANDOM_TICKS_PER_CHUNK {
                let lx = rng.draw().range(0, size - 1);
                let ly = rng.draw().range(0, size - 1);
                let pos = CellPos::new(base.x + lx, base.y + ly);
                rules::random_tick(window, pos, tick);
            }
        }
    }
}
