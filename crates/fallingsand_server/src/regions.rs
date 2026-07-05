use crate::persistence::{PlayerRecord, WorldStore, encode_region};
use crate::session::Sessions;
use crate::systems::PhysicsBody;
use crate::{INTEREST_RADIUS_X, INTEREST_RADIUS_Y, SimWorld};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellPos, ChunkOffset, ChunkPos, REGION_SIZE_CHUNKS, Region, RegionPos};
use fallingsand_sim::CellWorld;
use fallingsand_worldgen::WorldGenerator;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub const BORDER_MARGIN: i32 = 3;
pub const UNLOAD_GRACE_TICKS: u64 = 300;
pub const AUTOSAVE_INTERVAL_TICKS: u64 = 600;
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

pub fn wanted_regions(query: &Query<&PhysicsBody>) -> FxHashSet<RegionPos> {
    let mut wanted = FxHashSet::default();
    for body in query.iter() {
        let center = CellPos::new(body.0.x as i32, body.0.y as i32).chunk();
        let min = center.translated(
            -(INTEREST_RADIUS_X + BORDER_MARGIN),
            -(INTEREST_RADIUS_Y + BORDER_MARGIN),
        );
        let max = center.translated(
            INTEREST_RADIUS_X + BORDER_MARGIN,
            INTEREST_RADIUS_Y + BORDER_MARGIN,
        );
        for region_y in (min.y >> 3)..=(max.y >> 3) {
            for region_x in (min.x >> 3)..=(max.x >> 3) {
                wanted.insert(RegionPos::new(region_x, region_y));
            }
        }
    }
    wanted
}

fn insert_region(sim: &mut CellWorld, pos: RegionPos, region: Region) {
    let base = pos.base_chunk();
    let chunks = *region.into_chunks();
    for (index, chunk) in chunks.into_iter().enumerate() {
        let offset = ChunkOffset::from_index(index);
        sim.insert_chunk(
            ChunkPos::new(base.x + offset.x as i32, base.y + offset.y as i32),
            chunk,
        );
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

pub fn manage_regions(
    mut sim: ResMut<SimWorld>,
    mut regions: ResMut<RegionMap>,
    generator: Res<Generator>,
    store: Res<Store>,
    query: Query<&PhysicsBody>,
) {
    let tick = sim.0.tick();
    let wanted = wanted_regions(&query);

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
        insert_region(&mut sim.0, pos, region);
        regions.states.insert(
            pos,
            RegionState {
                dirty: false,
                last_wanted: tick,
            },
        );
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

    let expired: Vec<RegionPos> = regions
        .states
        .iter()
        .filter(|(pos, state)| {
            !wanted.contains(pos) && tick.saturating_sub(state.last_wanted) > UNLOAD_GRACE_TICKS
        })
        .map(|(&pos, _)| pos)
        .collect();

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
    query: Query<(&crate::session::Player, &PhysicsBody)>,
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

    let players: Vec<(String, PlayerRecord)> = sessions
        .sessions
        .iter()
        .filter_map(|session| {
            let entity = session.entity?;
            let (player, body) = query.get(entity).ok()?;
            Some((
                player.name.clone(),
                PlayerRecord {
                    x: body.0.x,
                    y: body.0.y,
                },
            ))
        })
        .collect();
    if let Err(err) = store.save_players(&players) {
        tracing::error!("player autosave failed: {err}");
    }
}

pub fn save_everything(world: &mut bevy_ecs::world::World) {
    let store = match &world.resource::<Store>().0 {
        Some(store) => store.clone(),
        None => return,
    };
    let started = std::time::Instant::now();
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

    let mut players: Vec<(String, PlayerRecord)> = Vec::new();
    {
        let mut query = world.query::<(&crate::session::Player, &PhysicsBody)>();
        for (player, body) in query.iter(world) {
            players.push((
                player.name.clone(),
                PlayerRecord {
                    x: body.0.x,
                    y: body.0.y,
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
