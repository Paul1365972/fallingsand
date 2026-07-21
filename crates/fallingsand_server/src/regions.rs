use crate::bodies::PixelBodies;
use crate::persistence::{Persistence, RegionReady, StoreError};
use crate::player::{Players, SearchWindow};
use crate::{INTEREST_RADIUS_X, INTEREST_RADIUS_Y};
use fallingsand_core::{CellPos, Chunk, ChunkPos, REGION_SIZE_CELLS, Region, RegionPos};
use fallingsand_sim::bodies::settle_body_quiet;
use fallingsand_sim::{CellWorld, PixelBody};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub const BORDER_MARGIN: i32 = 3;
pub const UNLOAD_GRACE_SECS: f32 = 5.0;
pub const UNLOAD_GRACE_TICKS: u64 = fallingsand_core::ticks_from_secs(UNLOAD_GRACE_SECS);
pub const MAX_LOADS_PER_TICK: usize = 1;
const MAX_PENDING_REGION_LOADS: usize = 64;

pub struct RegionState {
    pub revision: u64,
    pub persisted_revision: u64,
    pub last_wanted: u64,
}

#[derive(Default)]
pub struct RegionMap {
    pub states: FxHashMap<RegionPos, RegionState>,
    requested: FxHashMap<RegionPos, u64>,
    ready: Vec<RegionReady>,
}

impl RegionMap {
    pub fn counts(&self) -> (u32, u32) {
        let loaded = self.states.len() as u32;
        let dirty = self
            .states
            .values()
            .filter(|state| state.revision > state.persisted_revision)
            .count() as u32;
        (loaded, dirty)
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

pub fn compute_tickets(tickets: &mut ChunkTickets, spawn: CellPos, players: &Players) {
    tickets.active.clear();
    tickets.border.clear();
    tickets.random_tick.clear();
    for (_, player) in players.iter() {
        add_view(tickets, player.view_anchor().chunk());
        if let Some(materialization) = player.life.materialization() {
            add_search_window(tickets, materialization.search.window());
        }
    }
    let center = spawn.chunk();
    for dy in -INTEREST_RADIUS_Y..=INTEREST_RADIUS_Y {
        for dx in -INTEREST_RADIUS_X..=INTEREST_RADIUS_X {
            tickets.active.insert(center.translated(dx, dy));
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

fn snapshot_region(sim: &CellWorld, pos: RegionPos) -> Region {
    gather_region(pos, |chunk_pos| sim.chunk(chunk_pos).cloned())
}

pub(crate) fn collect_region_saves(
    sim: &CellWorld,
    regions: &RegionMap,
) -> Vec<(RegionPos, u64, u64, Arc<Region>)> {
    let mut out = Vec::new();
    for (pos, state) in &regions.states {
        if state.revision <= state.persisted_revision {
            continue;
        }
        out.push((
            *pos,
            state.revision,
            state.persisted_revision,
            Arc::new(snapshot_region(sim, *pos)),
        ));
    }
    out
}

pub(crate) fn mark_saved(
    regions: &mut RegionMap,
    positions: impl IntoIterator<Item = (RegionPos, u64)>,
) {
    for (pos, revision) in positions {
        if let Some(state) = regions.states.get_mut(&pos) {
            state.persisted_revision = state.persisted_revision.max(revision.min(state.revision));
        }
    }
}

fn body_region_radius(body: &PixelBody) -> i32 {
    ((body.width() as f32).hypot(body.height() as f32) + 1.0).ceil() as i32
}

fn body_overlaps_region(body: &PixelBody, pos: RegionPos) -> bool {
    let radius = body_region_radius(body);
    let base = pos.base_chunk().base_cell();
    let (cx, cy) = (body.x.floor_cell(), body.y.floor_cell());
    cx + radius > base.x
        && cx - radius < base.x + REGION_SIZE_CELLS as i32
        && cy + radius > base.y
        && cy - radius < base.y + REGION_SIZE_CELLS as i32
}

pub fn manage_regions(
    sim: &mut CellWorld,
    regions: &mut RegionMap,
    persistence: &mut Persistence,
    tickets: &ChunkTickets,
    bodies: &mut PixelBodies,
) -> Result<(), StoreError> {
    let tick = sim.tick();
    let wanted = wanted_regions(tickets);

    let mut completions = persistence.drain_completions()?;
    for &(pos, revision) in &completions.saved_regions {
        for ready in regions.ready.iter_mut().chain(&mut completions.regions) {
            if ready.pos == pos
                && let Ok(load) = &mut ready.result
            {
                load.persisted_revision = load.persisted_revision.max(revision.min(load.revision));
            }
        }
    }
    mark_saved(regions, completions.saved_regions);
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
        regions.states.insert(
            ready.pos,
            RegionState {
                revision: load.revision,
                persisted_revision: load.persisted_revision,
                last_wanted: tick,
            },
        );
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

    if !expired.is_empty() {
        let mut index = 0;
        while index < bodies.bodies.len() {
            let overlaps_expired = expired
                .iter()
                .copied()
                .any(|pos| body_overlaps_region(&bodies.bodies[index], pos));
            if overlaps_expired {
                let body = bodies.bodies.swap_remove(index);
                bodies.mark_owners_stale();
                settle_body_quiet(sim, &body);
            } else {
                index += 1;
            }
        }
    }

    mark_changed_regions(sim, regions);

    for pos in expired {
        let state = regions.states.remove(&pos).expect("state exists");
        let region = extract_region(sim, pos);
        persistence.stage_region(
            pos,
            state.revision,
            state.persisted_revision,
            Arc::new(region),
        );
    }
    persistence.pump()?;
    Ok(())
}

pub(crate) fn mark_changed_regions(sim: &CellWorld, regions: &mut RegionMap) {
    for (pos, state) in &mut regions.states {
        for (_, chunk_pos) in pos.chunk_positions() {
            if sim
                .chunk(chunk_pos)
                .is_some_and(|chunk| !chunk.change_rect().is_empty())
            {
                state.revision = state.revision.max(sim.tick().max(1));
                break;
            }
        }
    }
}
