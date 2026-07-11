use crate::inventory::Inventory;
use crate::player::{Air, Burning, Control, Health, Mode, Player, PlayerActor, PlayerRaster};
use crate::session::{SessionState, Sessions};
use crate::{INTEREST_RADIUS_X, INTEREST_RADIUS_Y, SimWorld, TickStats};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellOffset, ChunkPos};
use fallingsand_protocol::{
    ChunkDebugRects, ChunkOp, PlayerId, PlayerState, SelfState, ServerMessage, TickFrame,
    cells_to_wire,
};
use rustc_hash::FxHashSet;

#[derive(Resource, Default)]
pub struct LastPlayers(pub rustc_hash::FxHashMap<PlayerId, PlayerState>);

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn replicate(
    mut sessions: ResMut<Sessions>,
    sim: Res<SimWorld>,
    clock: Res<crate::WorldClock>,
    regions: Res<crate::regions::RegionMap>,
    mut last_players: ResMut<LastPlayers>,
    mut stats: ResMut<TickStats>,
    query: Query<(
        &Player,
        &PlayerActor,
        &PlayerRaster,
        &Control,
        &Health,
        &Mode,
        &Burning,
        &Air,
    )>,
    mut inventories: Query<&mut Inventory>,
) {
    let tick = sim.0.tick();
    let world_age = clock.0.age;

    let mut all_players: Vec<PlayerState> = query
        .iter()
        .map(
            |(player, body, raster, control, _, _, burning, _)| PlayerState {
                player: player.id,
                cx: body.0.x.floor_cell(),
                cy: body.0.y.floor_cell(),
                ducking: control.0.ducking(),
                burning: burning.active(),
                facing_left: raster.0.facing_left(),
            },
        )
        .collect();
    all_players.sort_unstable_by_key(|state| state.player.0);
    let changed_players: Vec<PlayerState> = all_players
        .iter()
        .filter(|state| last_players.0.get(&state.player) != Some(*state))
        .copied()
        .collect();
    last_players.0 = all_players
        .iter()
        .map(|state| (state.player, *state))
        .collect();

    for session in &mut sessions.sessions {
        if !matches!(session.state, SessionState::Playing) {
            continue;
        }
        let Some(entity) = session.entity else {
            continue;
        };
        let Ok((_, body, _, _, health, mode, _, air)) = query.get(entity) else {
            continue;
        };

        let center = body.0.cell().chunk();
        let mut interest = FxHashSet::default();
        for dy in -INTEREST_RADIUS_Y..=INTEREST_RADIUS_Y {
            for dx in -INTEREST_RADIUS_X..=INTEREST_RADIUS_X {
                let pos = center.translated(dx, dy);
                if sim.0.chunk(pos).is_some() {
                    interest.insert(pos);
                }
            }
        }

        let mut debug = Vec::new();
        let chunks = build_tiles(
            &mut session.known_chunks,
            session.debug,
            &sim.0,
            &interest,
            &mut debug,
        );
        let players = if session.fresh {
            all_players.clone()
        } else {
            changed_players.clone()
        };
        let (inventory, cursor, trash) = match inventories.get_mut(entity) {
            Ok(mut inv) => inv.delta(session.fresh),
            Err(_) => (Vec::new(), None, None),
        };
        let current_self = SelfState {
            hp: health.hp,
            air: air.secs,
            mode: mode.0,
        };
        let self_state = if session.last_self != Some(current_self) {
            session.last_self = Some(current_self);
            Some(current_self)
        } else {
            None
        };

        session.fresh = false;
        session.send(&ServerMessage::TickFrame(TickFrame {
            tick,
            world_age,
            chunks,
            players,
            inventory,
            cursor,
            trash,
            self_state,
            debug,
        }));
    }

    stats.players = all_players.len();
    (stats.awake_chunks, stats.awake_cells) = sim.0.awake_counts();
    stats.loaded_chunks = sim.0.chunk_count();
    (stats.loaded_regions, stats.dirty_regions) = regions.counts();
    stats.replicated_bytes = sessions.sessions.iter().map(|s| s.sent_bytes).sum();
    for session in &mut sessions.sessions {
        session.sent_bytes = 0;
    }
}

fn build_tiles(
    known: &mut FxHashSet<ChunkPos>,
    debug: bool,
    sim: &fallingsand_sim::CellWorld,
    interest: &FxHashSet<ChunkPos>,
    debug_rects: &mut Vec<ChunkDebugRects>,
) -> Vec<ChunkOp> {
    let mut ops = Vec::new();
    known.retain(|&pos| {
        if interest.contains(&pos) {
            return true;
        }
        ops.push(ChunkOp::Unload { pos });
        false
    });
    for &pos in interest {
        let chunk = sim.chunk(pos).expect("interest chunks are loaded");
        if debug {
            let change = chunk.change_rect();
            let sim = chunk.sim_rect();
            if !sim.is_empty() {
                debug_rects.push(ChunkDebugRects { pos, change, sim });
            }
        }
        if known.insert(pos) {
            ops.push(ChunkOp::Load {
                pos,
                cells: cells_to_wire(chunk.cells()),
            });
            continue;
        }
        let rect = chunk.change_rect();
        if rect.is_empty() {
            continue;
        }
        let mut cells = Vec::with_capacity((rect.width() * rect.height()) as usize);
        for y in rect.min_y..=rect.max_y {
            for x in rect.min_x..=rect.max_x {
                cells.push(chunk.get(CellOffset::new(x, y)));
            }
        }
        ops.push(ChunkOp::Delta {
            pos,
            rect,
            cells: cells_to_wire(&cells),
        });
    }
    ops
}

pub fn advance_clock(mut clock: ResMut<crate::WorldClock>) {
    clock.0.advance();
}
