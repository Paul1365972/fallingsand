use crate::rules;
use crate::window::{SimWindow, WINDOW_CHUNKS, WindowEvents};
use crate::world::CellWorld;
use fallingsand_core::{CHUNK_SIZE, CellPos, Chunk, ChunkPos, DirtyRect};
use fallingsand_math::Hash;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::time::Instant;

const _: () = assert!(WINDOW_CHUNKS == 4);
const RANDOM_TICKS_PER_CHUNK: u32 = 4;
const RANDOM_TICK_SAMPLE_SALT: Hash = Hash::label("simulation.random_tick_sample");
const PHASE_ORDER_SALT: Hash = Hash::label("simulation.phase_order");
const ROW_DIRECTION_SALT: Hash = Hash::label("simulation.row_direction");

pub(crate) fn row_reverse(tick: u64, y: i32) -> bool {
    Hash::seed(tick).salt(ROW_DIRECTION_SALT).pos(0, y).bit()
}

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

#[derive(Debug, Default, Clone, Copy)]
pub struct SimTimings {
    pub simulate_micros: u32,
    pub random_tick_micros: u32,
}

#[derive(Default)]
pub struct Simulator {
    ready: FxHashSet<ChunkPos>,
    origins: Vec<ChunkPos>,
    members: FxHashMap<ChunkPos, (usize, i32, i32)>,
    events: Vec<WindowEvents>,
}

impl Simulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn step_scoped<S, R>(
        &mut self,
        world: &mut CellWorld,
        simulate: &S,
        random_tick: &R,
    ) -> SimTimings
    where
        S: Fn(ChunkPos) -> bool + Sync,
        R: Fn(ChunkPos) -> bool + Sync,
    {
        world.advance_tick();
        let tick = world.tick();

        self.ready.clear();
        self.ready.extend(world.chunks().filter_map(|(pos, _)| {
            (simulate(pos)
                && (-1..=1)
                    .all(|dy| (-1..=1).all(|dx| world.chunk(pos.translated(dx, dy)).is_some())))
            .then_some(pos)
        }));
        let ready = &self.ready;
        world
            .chunk_map_mut()
            .par_iter_mut()
            .for_each(|(pos, chunk)| chunk.begin_tick(ready.contains(pos)));

        let awake = |pos: ChunkPos, chunk: &Chunk| simulate(pos) && !chunk.sim_rect().is_empty();
        let effect_micros = self.run_sim("effects", world, tick, &awake, &|window| {
            simulate_block(window, tick, simulate, rules::effect_cell)
        });
        let movement_micros = self.run_sim("movement", world, tick, &awake, &|window| {
            simulate_block(window, tick, simulate, rules::move_cell)
        });

        let random_tick_micros = self.run_sim(
            "random_tick",
            world,
            tick,
            &|pos, _| random_tick(pos),
            &|window| random_tick_block(window, tick, random_tick),
        );

        SimTimings {
            simulate_micros: effect_micros + movement_micros,
            random_tick_micros,
        }
    }

    fn run_sim<S, K>(
        &mut self,
        name: &'static str,
        world: &mut CellWorld,
        tick: u64,
        schedule: &S,
        kernel: &K,
    ) -> u32
    where
        S: Fn(ChunkPos, &Chunk) -> bool,
        K: Fn(&mut SimWindow) + Sync,
    {
        let _span = tracing::info_span!("sim", name).entered();
        let start = Instant::now();
        let order = Hash::seed(tick)
            .salt(PHASE_ORDER_SALT)
            .choose(&PHASE_ORDERS);
        for phase in order {
            self.run_phase(world, phase, schedule, kernel);
        }
        start.elapsed().as_micros() as u32
    }

    fn run_phase<S, K>(&mut self, world: &mut CellWorld, phase: u32, schedule: &S, kernel: &K)
    where
        S: Fn(ChunkPos, &Chunk) -> bool,
        K: Fn(&mut SimWindow) + Sync,
    {
        let _span = tracing::info_span!("sim_phase", phase).entered();
        let px = (phase & 1) as i32;
        let py = ((phase >> 1) & 1) as i32;

        let map = world.chunk_map_mut();
        self.origins.clear();
        self.origins.extend(map.iter().filter_map(|(pos, chunk)| {
            let block = (pos.x >> 1, pos.y >> 1);
            ((block.0 & 1) == px && (block.1 & 1) == py && schedule(*pos, chunk))
                .then(|| ChunkPos::new(((pos.x >> 1) << 1) - 1, ((pos.y >> 1) << 1) - 1))
        }));
        self.origins.sort_unstable();
        self.origins.dedup();

        self.members.clear();
        self.members.reserve(
            self.origins
                .len()
                .saturating_mul(WINDOW_CHUNKS as usize)
                .saturating_mul(WINDOW_CHUNKS as usize),
        );
        for (index, &origin) in self.origins.iter().enumerate() {
            for sy in 0..WINDOW_CHUNKS {
                for sx in 0..WINDOW_CHUNKS {
                    self.members
                        .insert(origin.translated(sx, sy), (index, sx, sy));
                }
            }
        }

        self.events
            .resize_with(self.origins.len(), WindowEvents::default);
        for events in &mut self.events[..self.origins.len()] {
            events.clear();
        }
        let mut windows: Vec<SimWindow> = self
            .origins
            .iter()
            .copied()
            .zip(self.events.iter_mut())
            .map(|(origin, events)| SimWindow::new(origin, std::array::from_fn(|_| None), events))
            .collect();
        for (&pos, chunk) in map.iter_mut() {
            if let Some(&(index, sx, sy)) = self.members.get(&pos) {
                windows[index].set_slot(sx, sy, chunk);
            }
        }

        windows.par_iter_mut().for_each(kernel);
        drop(windows);
        for events in &mut self.events[..self.origins.len()] {
            world.push_detachment_checks(events.drain_detachment_checks());
        }
    }
}

fn owned_chunks<S>(window: &SimWindow, simulate: &S) -> [[bool; 2]; 2]
where
    S: Fn(ChunkPos) -> bool + Sync,
{
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

fn simulate_block<S>(
    window: &mut SimWindow,
    tick: u64,
    simulate: &S,
    rule: fn(&mut SimWindow, CellPos, u64),
) where
    S: Fn(ChunkPos) -> bool + Sync,
{
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
        let reverse = row_reverse(tick, origin_cell.y + gy);
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
                rule(window, pos, tick);
            }
        }
    }
}

fn random_tick_block<S>(window: &mut SimWindow, tick: u64, simulate: &S)
where
    S: Fn(ChunkPos) -> bool + Sync,
{
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
