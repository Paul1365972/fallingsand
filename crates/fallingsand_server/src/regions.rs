use crate::persistence::{PlayerRecord, WorldStore, encode_region};
use crate::session::Sessions;
use crate::systems::{Health, PhysicsBody};
use crate::{INTEREST_RADIUS_X, INTEREST_RADIUS_Y, SimWorld};
use bevy_ecs::prelude::*;
use fallingsand_core::{
    CellOffset, CellPos, ChunkOffset, ChunkPos, MaterialRegistry, REGION_SIZE_CELLS,
    REGION_SIZE_CHUNKS, Region, RegionPos,
};
use fallingsand_protocol::PlayerUuid;
use fallingsand_sim::bodies::stamp_body;
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
        let center = CellPos::new(body.0.x as i32, body.0.y as i32).chunk();
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

fn insert_region(sim: &mut CellWorld, registry: &MaterialRegistry, pos: RegionPos, region: Region) {
    let base = pos.base_chunk();
    let chunks = *region.into_chunks();
    let mut reactive: Vec<CellPos> = Vec::new();
    for (index, chunk) in chunks.into_iter().enumerate() {
        let offset = ChunkOffset::from_index(index);
        let chunk_pos = ChunkPos::new(base.x + offset.x as i32, base.y + offset.y as i32);
        for (cell_index, cell) in chunk.cells().iter().enumerate() {
            if registry.is_reactive(cell.material) {
                reactive.push(chunk_pos.cell(CellOffset::from_index(cell_index)));
            }
        }
        sim.insert_chunk(chunk_pos, chunk);
    }
    for cell_pos in reactive {
        sim.mark_keep(cell_pos);
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
    let radius = (body.width as f32).hypot(body.height as f32) + 1.0;
    let base = pos.base_chunk().base_cell();
    let (min_x, min_y) = (base.x as f32, base.y as f32);
    let max_x = min_x + REGION_SIZE_CELLS as f32;
    let max_y = min_y + REGION_SIZE_CELLS as f32;
    body.x + radius > min_x
        && body.x - radius < max_x
        && body.y + radius > min_y
        && body.y - radius < max_y
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
        let region = loaded.unwrap_or_else(|| generator.0.generate_region(pos));
        insert_region(&mut sim.0, &registry.0, pos, region);
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
                stamp_body(&mut sim.0, &registry.0, &body);
                bodies.despawned.push(body.id);
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

pub fn autosave(
    sim: Res<SimWorld>,
    mut regions: ResMut<RegionMap>,
    store: Res<Store>,
    sessions: Res<Sessions>,
    query: Query<(&crate::session::Player, &PhysicsBody, &Health)>,
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
            let (player, body, health) = query.get(entity).ok()?;
            Some((
                player.uuid,
                PlayerRecord {
                    x: body.0.x,
                    y: body.0.y,
                    hp: health.hp,
                },
            ))
        })
        .collect();
    if let Err(err) = store.save_players(&players) {
        tracing::error!("player autosave failed: {err}");
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
                for body in &bodies.bodies {
                    stamp_body(&mut sim.0, &registry, body);
                    let radius = (body.width as f32).hypot(body.height as f32) + 1.0;
                    let min = CellPos::new(
                        (body.x - radius).floor() as i32,
                        (body.y - radius).floor() as i32,
                    )
                    .region();
                    let max = CellPos::new(
                        (body.x + radius).ceil() as i32,
                        (body.y + radius).ceil() as i32,
                    )
                    .region();
                    for region_y in min.y..=max.y {
                        for region_x in min.x..=max.x {
                            touched.insert(RegionPos::new(region_x, region_y));
                        }
                    }
                }
            }
            for body in bodies.bodies.drain(..) {
                bodies.despawned.push(body.id);
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
        let mut query = world.query::<(&crate::session::Player, &PhysicsBody, &Health)>();
        for (player, body, health) in query.iter(world) {
            players.push((
                player.uuid,
                PlayerRecord {
                    x: body.0.x,
                    y: body.0.y,
                    hp: health.hp,
                },
            ));
        }
    }
    if let Err(err) = store.save_players(&players) {
        tracing::error!("final player save failed: {err}");
    }
    tracing::info!(
        "world saved: {} regions, {} players in {:.1?}",
        to_save.len(),
        players.len(),
        started.elapsed(),
    );
}
