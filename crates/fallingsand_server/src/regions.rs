use crate::bodies::BodyWorld;
use crate::persistence::{Persistence, RegionReady, StoreError};
use crate::player::{Players, SearchWindow};
use crate::{INTEREST_RADIUS_X, INTEREST_RADIUS_Y};
use fallingsand_core::{Chunk, ChunkPos, Region, RegionPos};
use fallingsand_sim::CellWorld;
use rustc_hash::{FxHashMap, FxHashSet};

pub const BORDER_MARGIN: i32 = 3;
pub const UNLOAD_GRACE_SECS: f32 = 5.0;
pub const UNLOAD_GRACE_TICKS: u64 = fallingsand_core::ticks_from_secs(UNLOAD_GRACE_SECS);
pub const MAX_LOADS_PER_TICK: usize = 1;
const MAX_PENDING_REGION_LOADS: usize = 64;

struct RegionState {
    last_wanted: u64,
}

#[derive(Default)]
pub struct RegionMap {
    states: FxHashMap<RegionPos, RegionState>,
    requested: FxHashMap<RegionPos, u64>,
    ready: Vec<RegionReady>,
}

impl RegionMap {
    pub fn len(&self) -> usize {
        self.states.len()
    }
}

#[derive(Default)]
pub struct ChunkTickets {
    pub active: FxHashSet<ChunkPos>,
    pub border: FxHashSet<ChunkPos>,
    pub random_tick: FxHashSet<ChunkPos>,
}

impl ChunkTickets {
    pub fn simulates(&self, pos: ChunkPos) -> bool {
        self.active.contains(&pos) || self.border.contains(&pos)
    }

    pub fn random_ticks(&self, pos: ChunkPos) -> bool {
        self.random_tick.contains(&pos)
    }
}

pub fn compute_tickets(tickets: &mut ChunkTickets, players: &Players) {
    tickets.active.clear();
    tickets.border.clear();
    tickets.random_tick.clear();
    for (_, player) in players.iter() {
        add_view(tickets, player.view_anchor().chunk());
        if let Some(materialization) = player.life.materialization() {
            add_search_window(tickets, materialization.search.window());
        }
    }
    tickets.border.retain(|pos| !tickets.active.contains(pos));
}

fn add_view(tickets: &mut ChunkTickets, center: ChunkPos) {
    for dy in -(INTEREST_RADIUS_Y + BORDER_MARGIN)..=(INTEREST_RADIUS_Y + BORDER_MARGIN) {
        for dx in -(INTEREST_RADIUS_X + BORDER_MARGIN)..=(INTEREST_RADIUS_X + BORDER_MARGIN) {
            let pos = center.translated(dx, dy);
            if dx.abs() <= INTEREST_RADIUS_X && dy.abs() <= INTEREST_RADIUS_Y {
                tickets.active.insert(pos);
                tickets.random_tick.insert(pos);
            } else {
                tickets.border.insert(pos);
            }
        }
    }
}

fn add_search_window(tickets: &mut ChunkTickets, window: SearchWindow) {
    let min = window.min.chunk();
    let max = window.max.chunk();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            tickets.active.insert(ChunkPos::new(x, y));
        }
    }
}

pub fn wanted_regions(tickets: &ChunkTickets) -> FxHashSet<RegionPos> {
    tickets
        .active
        .iter()
        .chain(tickets.border.iter())
        .map(|pos| pos.region())
        .collect()
}

fn insert_region(sim: &mut CellWorld, pos: RegionPos, region: Region) {
    for ((_, chunk_pos), chunk) in pos.chunk_positions().zip(*region.into_chunks()) {
        sim.insert_chunk(chunk_pos, chunk);
    }
}

fn gather_region(pos: RegionPos, mut chunk_of: impl FnMut(ChunkPos) -> Option<Chunk>) -> Region {
    let mut region = Region::new();
    for (offset, chunk_pos) in pos.chunk_positions() {
        if let Some(chunk) = chunk_of(chunk_pos) {
            *region.chunk_mut(offset) = chunk;
        }
    }
    region
}

fn extract_region(sim: &mut CellWorld, pos: RegionPos) -> Region {
    gather_region(pos, |chunk_pos| sim.remove_chunk(chunk_pos))
}

pub(crate) fn snapshot_regions(
    sim: &CellWorld,
    regions: &RegionMap,
) -> Vec<(RegionPos, std::sync::Arc<Region>)> {
    regions
        .states
        .keys()
        .map(|&pos| {
            let region = gather_region(pos, |chunk_pos| {
                Some(
                    sim.chunk(chunk_pos)
                        .expect("loaded region owns all of its chunks")
                        .clone(),
                )
            });
            (pos, region.into())
        })
        .collect()
}

pub fn manage_regions(
    sim: &mut CellWorld,
    regions: &mut RegionMap,
    persistence: &mut Persistence,
    tickets: &ChunkTickets,
    bodies: &mut BodyWorld,
) -> Result<(), StoreError> {
    let tick = sim.tick();
    let wanted = wanted_regions(tickets);

    let completions = persistence.drain_completions()?;
    regions.ready.extend(completions.regions);
    regions
        .ready
        .sort_unstable_by_key(|ready| (ready.pos.y, ready.pos.x));

    let mut integrated = 0;
    let mut index = 0;
    while index < regions.ready.len() {
        let ready = &regions.ready[index];
        let requested = regions.requested.get(&ready.pos).copied();
        if requested != Some(ready.request) {
            regions.ready.remove(index).result?;
            continue;
        }
        if !wanted.contains(&ready.pos) {
            regions.requested.remove(&ready.pos);
            regions.ready.remove(index).result?;
            continue;
        }
        if integrated >= MAX_LOADS_PER_TICK {
            index += 1;
            continue;
        }
        let ready = regions.ready.remove(index);
        let load = ready.result?;
        regions.requested.remove(&ready.pos);
        insert_region(sim, ready.pos, load.region);
        regions
            .states
            .insert(ready.pos, RegionState { last_wanted: tick });
        integrated += 1;
    }

    for pos in &wanted {
        if let Some(state) = regions.states.get_mut(pos) {
            state.last_wanted = tick;
        }
    }

    let mut candidates: Vec<_> = wanted.iter().copied().collect();
    candidates.sort_unstable_by_key(|pos| (pos.y, pos.x));
    for pos in candidates {
        if regions.requested.len() >= MAX_PENDING_REGION_LOADS {
            break;
        }
        if regions.states.contains_key(&pos) || regions.requested.contains_key(&pos) {
            continue;
        }
        let (request, ready) = persistence.request_region(pos)?;
        regions.requested.insert(pos, request);
        if let Some(load) = ready {
            regions.ready.push(RegionReady {
                request,
                pos,
                result: Ok(load),
            });
        }
    }

    let mut expired: Vec<RegionPos> = regions
        .states
        .iter()
        .filter(|(pos, state)| {
            !wanted.contains(pos) && tick.saturating_sub(state.last_wanted) > UNLOAD_GRACE_TICKS
        })
        .map(|(&pos, _)| pos)
        .collect();
    expired.sort_unstable_by_key(|pos| (pos.y, pos.x));

    bodies.settle_overlapping_regions(sim, &expired);

    for pos in expired {
        regions.states.remove(&pos).expect("state exists");
        let region = extract_region(sim, pos);
        persistence.stage_region(pos, region);
    }
    Ok(())
}
