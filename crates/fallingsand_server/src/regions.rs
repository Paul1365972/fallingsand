use crate::persistence::{PlayerRecord, WorldMeta, WorldStore, encode_region};
use crate::session::Sessions;
use crate::systems::{Air, Burning, Health, Inventory, Mode, PhysicsBody, player_record};
use crate::{INTEREST_RADIUS_X, INTEREST_RADIUS_Y, SimWorld, WorldClock, WorldInfo};
use bevy_ecs::prelude::*;
use fallingsand_core::{
    CellPos, ChunkOffset, ChunkPos, REGION_SIZE_CELLS, REGION_SIZE_CHUNKS, Region, RegionPos,
};
use fallingsand_protocol::PlayerUuid;
use fallingsand_sim::bodies::settle_body;
use fallingsand_sim::{CellWorld, PixelBody};
use fallingsand_worldgen::WorldGenerator;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub const BORDER_MARGIN: i32 = 3;
pub const UNLOAD_GRACE_SECS: f32 = 5.0;
pub const AUTOSAVE_INTERVAL_SECS: f32 = 10.0;
pub const UNLOAD_GRACE_TICKS: u64 = (UNLOAD_GRACE_SECS * crate::TICK_RATE as f32) as u64;
pub const AUTOSAVE_INTERVAL_TICKS: u64 = (AUTOSAVE_INTERVAL_SECS * crate::TICK_RATE as f32) as u64;
pub const MAX_LOADS_PER_TICK: usize = 1;

#[derive(Resource)]
pub struct Generator(pub Arc<WorldGenerator>);

#[derive(Resource)]
pub struct Store(pub Option<Arc<WorldStore>>);

pub struct RegionState {
    pub dirty: bool,
    pub last_wanted: u64,
}

#[derive(Resource, Default)]
pub struct RegionMap {
    pub states: FxHashMap<RegionPos, RegionState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketLevel {
    Active,
    Border,
    Loaded,
}

#[derive(Resource, Default)]
pub struct ChunkTickets {
    pub active: FxHashSet<ChunkPos>,
    pub border: FxHashSet<ChunkPos>,
}

impl ChunkTickets {
    pub fn simulates(&self, pos: ChunkPos) -> bool {
        self.active.contains(&pos) || self.border.contains(&pos)
    }

    pub fn level(&self, pos: ChunkPos) -> TicketLevel {
        if self.active.contains(&pos) {
            TicketLevel::Active
        } else if self.border.contains(&pos) {
            TicketLevel::Border
        } else {
            TicketLevel::Loaded
        }
    }
}

pub fn compute_tickets(mut tickets: ResMut<ChunkTickets>, query: Query<&PhysicsBody>) {
    let ChunkTickets { active, border } = &mut *tickets;
    active.clear();
    border.clear();
    for body in query.iter() {
        let center = CellPos::new(body.0.x.floor_cell(), body.0.y.floor_cell()).chunk();
        for dy in -(INTEREST_RADIUS_Y + BORDER_MARGIN)..=(INTEREST_RADIUS_Y + BORDER_MARGIN) {
            for dx in -(INTEREST_RADIUS_X + BORDER_MARGIN)..=(INTEREST_RADIUS_X + BORDER_MARGIN) {
                let pos = center.translated(dx, dy);
                if dx.abs() <= INTEREST_RADIUS_X && dy.abs() <= INTEREST_RADIUS_Y {
                    active.insert(pos);
                } else {
                    border.insert(pos);
                }
            }
        }
    }
    border.retain(|pos| !active.contains(pos));
}

pub fn wanted_regions(tickets: &ChunkTickets) -> FxHashSet<RegionPos> {
    tickets
        .active
        .iter()
        .chain(tickets.border.iter())
        .map(|pos| pos.region())
        .collect()
}

fn strip_body_flags(region: &mut Region) {
    for index in 0..REGION_SIZE_CHUNKS * REGION_SIZE_CHUNKS {
        let chunk = region.chunk_mut(ChunkOffset::from_index(index));
        for cell in chunk.cells_mut().iter_mut() {
            if cell.is_body() {
                cell.set_body(false);
            }
        }
    }
}

fn insert_region(sim: &mut CellWorld, pos: RegionPos, region: Region) {
    let base = pos.base_chunk();
    let chunks = *region.into_chunks();
    for (index, chunk) in chunks.into_iter().enumerate() {
        let offset = ChunkOffset::from_index(index);
        let chunk_pos = ChunkPos::new(base.x + offset.x as i32, base.y + offset.y as i32);
        sim.insert_chunk(chunk_pos, chunk);
    }
}

fn extract_region(sim: &mut CellWorld, pos: RegionPos) -> Region {
    let base = pos.base_chunk();
    let mut region = Region::new();
    for index in 0..REGION_SIZE_CHUNKS * REGION_SIZE_CHUNKS {
        let offset = ChunkOffset::from_index(index);
        let chunk_pos = ChunkPos::new(base.x + offset.x as i32, base.y + offset.y as i32);
        if let Some(chunk) = sim.remove_chunk(chunk_pos) {
            *region.chunk_mut(offset) = chunk;
        }
    }
    region
}

fn body_overlaps_region(body: &PixelBody, pos: RegionPos) -> bool {
    let radius = ((body.width as f32).hypot(body.height as f32) + 1.0).ceil() as i32;
    let base = pos.base_chunk().base_cell();
    let (cx, cy) = (body.x.floor_cell(), body.y.floor_cell());
    cx + radius > base.x
        && cx - radius < base.x + REGION_SIZE_CELLS as i32
        && cy + radius > base.y
        && cy - radius < base.y + REGION_SIZE_CELLS as i32
}

#[allow(clippy::too_many_arguments)]
pub fn manage_regions(
    mut sim: ResMut<SimWorld>,
    mut regions: ResMut<RegionMap>,
    generator: Res<Generator>,
    store: Res<Store>,
    registry: Res<crate::Registry>,
    tickets: Res<ChunkTickets>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
) {
    let tick = sim.0.tick();
    let wanted = wanted_regions(&tickets);

    for pos in &wanted {
        if let Some(state) = regions.states.get_mut(pos) {
            state.last_wanted = tick;
        }
    }

    let mut loads = 0usize;
    for &pos in &wanted {
        if regions.states.contains_key(&pos) {
            continue;
        }
        if loads >= MAX_LOADS_PER_TICK {
            break;
        }
        loads += 1;
        let loaded = store.0.as_ref().and_then(|store| {
            store.load_region(pos).unwrap_or_else(|err| {
                tracing::error!("failed to load region {pos:?}: {err}");
                None
            })
        });
        let region = match loaded {
            Some(mut region) => {
                strip_body_flags(&mut region);
                region
            }
            None => generator.0.generate_region(pos),
        };
        insert_region(&mut sim.0, pos, region);
        regions.states.insert(
            pos,
            RegionState {
                dirty: false,
                last_wanted: tick,
            },
        );
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
        let bodies = &mut *bodies;
        let mut index = 0;
        while index < bodies.bodies.len() {
            let unloading = expired
                .iter()
                .any(|&pos| body_overlaps_region(&bodies.bodies[index], pos));
            if unloading {
                let body = bodies.bodies.swap_remove(index);
                settle_body(&mut sim.0, &registry.0, &[], &body, true);
            } else {
                index += 1;
            }
        }
    }

    for (pos, state) in regions.states.iter_mut() {
        if state.dirty {
            continue;
        }
        let base = pos.base_chunk();
        'scan: for index in 0..REGION_SIZE_CHUNKS * REGION_SIZE_CHUNKS {
            let offset = ChunkOffset::from_index(index);
            let chunk_pos = ChunkPos::new(base.x + offset.x as i32, base.y + offset.y as i32);
            if let Some(chunk) = sim.0.chunk(chunk_pos)
                && !chunk.dirty().is_empty()
            {
                state.dirty = true;
                break 'scan;
            }
        }
    }

    let mut to_save: Vec<(RegionPos, Vec<u8>)> = Vec::new();
    for pos in expired {
        let state = regions.states.remove(&pos).expect("state exists");
        let region = extract_region(&mut sim.0, pos);
        if state.dirty && store.0.is_some() {
            to_save.push((pos, encode_region(&region)));
        }
    }
    if let Some(store) = store.0.as_ref()
        && let Err(err) = store.save_regions(&to_save)
    {
        tracing::error!("failed to save {} regions: {err}", to_save.len());
    }
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn autosave(
    sim: Res<SimWorld>,
    mut regions: ResMut<RegionMap>,
    store: Res<Store>,
    sessions: Res<Sessions>,
    registry: Res<crate::Registry>,
    info: Res<WorldInfo>,
    clock: Res<WorldClock>,
    query: Query<(
        &crate::session::Player,
        &PhysicsBody,
        &Health,
        &Mode,
        &Air,
        &Burning,
        &Inventory,
    )>,
) {
    let Some(store) = store.0.as_ref() else {
        return;
    };
    let tick = sim.0.tick();
    if tick == 0 || !tick.is_multiple_of(AUTOSAVE_INTERVAL_TICKS) {
        return;
    }

    let mut to_save: Vec<(RegionPos, Vec<u8>)> = Vec::new();
    for (pos, state) in regions.states.iter_mut() {
        if !state.dirty {
            continue;
        }
        let base = pos.base_chunk();
        let mut region = Region::new();
        for index in 0..REGION_SIZE_CHUNKS * REGION_SIZE_CHUNKS {
            let offset = ChunkOffset::from_index(index);
            let chunk_pos = ChunkPos::new(base.x + offset.x as i32, base.y + offset.y as i32);
            if let Some(chunk) = sim.0.chunk(chunk_pos) {
                *region.chunk_mut(offset) = chunk.clone();
            }
        }
        to_save.push((*pos, encode_region(&region)));
        state.dirty = false;
    }
    match store.save_regions(&to_save) {
        Ok(()) if !to_save.is_empty() => tracing::debug!("autosaved {} regions", to_save.len()),
        Ok(()) => {}
        Err(err) => tracing::error!("autosave failed: {err}"),
    }

    let players: Vec<(PlayerUuid, PlayerRecord)> = sessions
        .sessions
        .iter()
        .filter_map(|session| {
            let entity = session.entity?;
            let (player, body, health, mode, air, burning, inventory) = query.get(entity).ok()?;
            Some((
                player.uuid,
                player_record(&registry.0, &body.0, health, mode, air, burning, inventory),
            ))
        })
        .collect();
    if let Err(err) = store.save_players(&players) {
        tracing::error!("player autosave failed: {err}");
    }
    if let Err(err) = store.save_meta(&world_meta(&info, &clock)) {
        tracing::error!("meta autosave failed: {err}");
    }
}

pub fn world_meta(info: &WorldInfo, clock: &WorldClock) -> WorldMeta {
    WorldMeta {
        format_version: crate::persistence::WORLD_FORMAT_VERSION,
        seed: info.seed,
        name: info.name.clone(),
        clock: clock.t,
        day: clock.day,
    }
}

pub fn save_everything(world: &mut bevy_ecs::world::World, final_save: bool) {
    let store = match &world.resource::<Store>().0 {
        Some(store) => store.clone(),
        None => return,
    };
    let started = std::time::Instant::now();

    if final_save {
        let registry = world.resource::<crate::Registry>().0.clone();
        let mut bodies = std::mem::take(&mut *world.resource_mut::<crate::bodies::PixelBodies>());
        if !bodies.bodies.is_empty() {
            let mut touched: FxHashSet<RegionPos> = FxHashSet::default();
            {
                let mut sim = world.resource_mut::<SimWorld>();
                for body in bodies.bodies.drain(..) {
                    settle_body(&mut sim.0, &registry, &[], &body, true);
                    let radius =
                        ((body.width as f32).hypot(body.height as f32) + 1.0).ceil() as i32;
                    let (cx, cy) = (body.x.floor_cell(), body.y.floor_cell());
                    let min = CellPos::new(cx - radius, cy - radius).region();
                    let max = CellPos::new(cx + radius + 1, cy + radius + 1).region();
                    for region_y in min.y..=max.y {
                        for region_x in min.x..=max.x {
                            touched.insert(RegionPos::new(region_x, region_y));
                        }
                    }
                }
            }
            let mut regions = world.resource_mut::<RegionMap>();
            for pos in touched {
                if let Some(state) = regions.states.get_mut(&pos) {
                    state.dirty = true;
                }
            }
        }
        *world.resource_mut::<crate::bodies::PixelBodies>() = bodies;
    }

    let mut to_save: Vec<(RegionPos, Vec<u8>)> = Vec::new();
    {
        let regions = world.resource::<RegionMap>();
        let sim = world.resource::<SimWorld>();
        for (pos, state) in regions.states.iter() {
            if !state.dirty {
                continue;
            }
            let base = pos.base_chunk();
            let mut region = Region::new();
            for index in 0..REGION_SIZE_CHUNKS * REGION_SIZE_CHUNKS {
                let offset = ChunkOffset::from_index(index);
                let chunk_pos = ChunkPos::new(base.x + offset.x as i32, base.y + offset.y as i32);
                if let Some(chunk) = sim.0.chunk(chunk_pos) {
                    *region.chunk_mut(offset) = chunk.clone();
                }
            }
            to_save.push((*pos, encode_region(&region)));
        }
    }
    if let Err(err) = store.save_regions(&to_save) {
        tracing::error!("final save failed: {err}");
    }
    for (pos, _) in &to_save {
        if let Some(state) = world.resource_mut::<RegionMap>().states.get_mut(pos) {
            state.dirty = false;
        }
    }

    let mut players: Vec<(PlayerUuid, PlayerRecord)> = Vec::new();
    {
        let registry = world.resource::<crate::Registry>().0.clone();
        let mut query = world.query::<(
            &crate::session::Player,
            &PhysicsBody,
            &Health,
            &Mode,
            &Air,
            &Burning,
            &Inventory,
        )>();
        for (player, body, health, mode, air, burning, inventory) in query.iter(world) {
            players.push((
                player.uuid,
                player_record(&registry, &body.0, health, mode, air, burning, inventory),
            ));
        }
    }
    if let Err(err) = store.save_players(&players) {
        tracing::error!("final player save failed: {err}");
    }
    let meta = world_meta(
        world.resource::<WorldInfo>(),
        world.resource::<WorldClock>(),
    );
    if let Err(err) = store.save_meta(&meta) {
        tracing::error!("final meta save failed: {err}");
    }
    tracing::info!(
        "world saved: {} regions, {} players in {:.1?}",
        to_save.len(),
        players.len(),
        started.elapsed(),
    );
}
