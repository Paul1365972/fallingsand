use crate::bodies::PixelBodies;
use crate::persistence::{Persistence, StoreError, encode_region};
use crate::player::{Players, SearchWindow};
use crate::{INTEREST_RADIUS_X, INTEREST_RADIUS_Y};
use fallingsand_core::{CellPos, Chunk, ChunkPos, REGION_SIZE_CELLS, Region, RegionPos};
use fallingsand_sim::bodies::settle_body;
use fallingsand_sim::{CellWorld, PixelBody};
use fallingsand_worldgen::WorldGenerator;
use rustc_hash::{FxHashMap, FxHashSet};

pub const BORDER_MARGIN: i32 = 3;
pub const UNLOAD_GRACE_SECS: f32 = 5.0;
pub const UNLOAD_GRACE_TICKS: u64 = fallingsand_core::ticks_from_secs(UNLOAD_GRACE_SECS);
pub const MAX_LOADS_PER_TICK: usize = 1;

pub struct RegionState {
    pub dirty: bool,
    pub last_wanted: u64,
}

#[derive(Default)]
pub struct RegionMap {
    pub states: FxHashMap<RegionPos, RegionState>,
}

impl RegionMap {
    pub fn counts(&self) -> (u32, u32) {
        let loaded = self.states.len() as u32;
        let dirty = self.states.values().filter(|state| state.dirty).count() as u32;
        (loaded, dirty)
    }
}

#[derive(Default)]
pub struct ChunkTickets {
    pub active: FxHashSet<ChunkPos>,
    pub border: FxHashSet<ChunkPos>,
}

impl ChunkTickets {
    pub fn simulates(&self, pos: ChunkPos) -> bool {
        self.active.contains(&pos) || self.border.contains(&pos)
    }
}

pub fn compute_tickets(tickets: &mut ChunkTickets, spawn: CellPos, players: &Players) {
    tickets.active.clear();
    tickets.border.clear();
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

fn strip_body_remnants(region: &mut Region) {
    for chunk in region.chunks_mut().iter_mut() {
        for cell in chunk.cells_mut().iter_mut() {
            if fallingsand_core::content::tags(cell.material)
                .contains(fallingsand_core::Tag::Player)
            {
                *cell = fallingsand_core::Cell::AIR;
            } else if cell.is_body() {
                cell.set_body(false);
            }
        }
    }
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

pub(crate) fn collect_dirty_saves(
    sim: &CellWorld,
    regions: &RegionMap,
) -> Vec<(RegionPos, Vec<u8>)> {
    let mut out = Vec::new();
    for (pos, state) in &regions.states {
        if !state.dirty {
            continue;
        }
        out.push((*pos, encode_region(&snapshot_region(sim, *pos))));
    }
    out
}

pub(crate) fn mark_saved(regions: &mut RegionMap, positions: impl IntoIterator<Item = RegionPos>) {
    for pos in positions {
        if let Some(state) = regions.states.get_mut(&pos) {
            state.dirty = false;
        }
    }
}

pub(crate) fn body_region_radius(body: &PixelBody) -> i32 {
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
    generator: &WorldGenerator,
    persistence: &mut Persistence,
    tickets: &ChunkTickets,
    bodies: &mut PixelBodies,
) -> Result<(), StoreError> {
    let tick = sim.tick();
    let wanted = wanted_regions(tickets);

    for pos in &wanted {
        if let Some(state) = regions.states.get_mut(pos) {
            state.last_wanted = tick;
        }
    }

    let mut candidates: Vec<_> = wanted.iter().copied().collect();
    candidates.sort_unstable_by_key(|pos| (pos.y, pos.x));
    let mut loads = 0usize;
    for pos in candidates {
        if regions.states.contains_key(&pos) || loads >= MAX_LOADS_PER_TICK {
            continue;
        }
        let loaded = persistence
            .load_region(pos)
            .map_err(|source| StoreError::RegionLoad {
                pos,
                source: Box::new(source),
            })?;
        let region = match loaded {
            Some(mut region) => {
                strip_body_remnants(&mut region);
                region
            }
            None => generator.generate_region(pos),
        };
        insert_region(sim, pos, region);
        regions.states.insert(
            pos,
            RegionState {
                dirty: false,
                last_wanted: tick,
            },
        );
        loads += 1;
    }

    let expired: Vec<RegionPos> = regions
        .states
        .iter()
        .filter(|(pos, state)| {
            !wanted.contains(pos) && tick.saturating_sub(state.last_wanted) > UNLOAD_GRACE_TICKS
        })
        .map(|(&pos, _)| pos)
        .collect();

    if !expired.is_empty() {
        let mut index = 0;
        while index < bodies.bodies.len() {
            let unloading = expired
                .iter()
                .any(|&pos| body_overlaps_region(&bodies.bodies[index], pos));
            if unloading {
                let body = bodies.bodies.swap_remove(index);
                settle_body(sim, &body);
            } else {
                index += 1;
            }
        }
    }

    mark_changed_regions(sim, regions);

    for pos in expired {
        regions.states.remove(&pos).expect("state exists");
        let region = extract_region(sim, pos);
        persistence.stage_region(pos, encode_region(&region));
    }
    if let Err(err) = persistence.flush_regions() {
        tracing::error!("failed to save unloaded regions: {err}");
    }
    Ok(())
}

pub(crate) fn mark_changed_regions(sim: &CellWorld, regions: &mut RegionMap) {
    for (pos, state) in &mut regions.states {
        if state.dirty {
            continue;
        }
        for (_, chunk_pos) in pos.chunk_positions() {
            if sim
                .chunk(chunk_pos)
                .is_some_and(|chunk| !chunk.change_rect().is_empty())
            {
                state.dirty = true;
                break;
            }
        }
    }
}
