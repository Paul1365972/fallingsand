use crate::inventory::Inventory;
use crate::player::{PlayerLife, Players};
use crate::regions::RegionMap;
use crate::session::Sessions;
use crate::{INTEREST_RADIUS_X, INTEREST_RADIUS_Y};
use fallingsand_core::{CHUNK_SIZE, Calendar, CellOffset, ChunkPos, ItemStack};
use fallingsand_protocol::{
    ChunkDebugRects, ChunkOp, InteractionState, InteractionStatus, ParticleSpawn,
    PlayerAvatarState, PlayerId, PlayerState, SelfAvatarState, SelfLife, SelfState, ServerMessage,
    ServerStats, TickFrame, cells_to_wire,
};
use fallingsand_sim::CellWorld;
use fallingsand_worldgen::WorldGenerator;
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;

pub struct SessionReplication {
    pub known_chunks: FxHashSet<ChunkPos>,
    pub last_self: Option<SelfState>,
    pub last_inventory: Vec<Option<ItemStack>>,
    pub last_cursor: Option<ItemStack>,
    pub last_trash: Option<ItemStack>,
    pub fresh: bool,
    pub sent_bytes: u64,
    pub debug: bool,
}

impl Default for SessionReplication {
    fn default() -> Self {
        Self {
            known_chunks: FxHashSet::default(),
            last_self: None,
            last_inventory: Vec::new(),
            last_cursor: None,
            last_trash: None,
            fresh: true,
            sent_bytes: 0,
            debug: false,
        }
    }
}

#[derive(Default)]
pub struct ReplicationState {
    last_players: BTreeMap<PlayerId, PlayerState>,
}

#[allow(clippy::too_many_arguments)]
pub fn replicate(
    sessions: &mut Sessions,
    players: &Players,
    sim: &CellWorld,
    clock: &Calendar,
    regions: &RegionMap,
    generator: &WorldGenerator,
    particles: &[ParticleSpawn],
    replication: &mut ReplicationState,
    stats: &mut ServerStats,
) {
    let tick = sim.tick();
    let all_players: Vec<PlayerState> = players
        .iter()
        .map(|(&id, player)| PlayerState {
            player: id,
            avatar: player.avatar().map(|avatar| PlayerAvatarState {
                cx: avatar.actor.x.floor_cell(),
                cy: avatar.actor.y.floor_cell(),
                height: avatar.actor.rows() as u8,
                burning: avatar.burning_secs > 0.0,
            }),
        })
        .collect();
    let changed_players: Vec<PlayerState> = all_players
        .iter()
        .filter(|state| replication.last_players.get(&state.player) != Some(*state))
        .copied()
        .collect();
    replication.last_players = all_players
        .iter()
        .map(|state| (state.player, *state))
        .collect();

    for session in sessions.active_iter_mut() {
        let Some(player_id) = session.player() else {
            continue;
        };
        let Some(player) = players.get(player_id) else {
            continue;
        };
        let center = player.view_anchor().chunk();
        let mut interest = FxHashSet::default();
        for dy in -INTEREST_RADIUS_Y..=INTEREST_RADIUS_Y {
            for dx in -INTEREST_RADIUS_X..=INTEREST_RADIUS_X {
                let pos = center.translated(dx, dy);
                if sim.chunk(pos).is_some() {
                    interest.insert(pos);
                }
            }
        }

        let mut debug = Vec::new();
        let chunks = build_tiles(
            &mut session.replication.known_chunks,
            session.replication.debug,
            sim,
            &interest,
            &mut debug,
        );
        let in_interest = particles_in_interest(particles, center);
        let public_players = if session.replication.fresh {
            all_players.clone()
        } else {
            changed_players.clone()
        };
        let fresh = session.replication.fresh;
        let inventory = inventory_delta(&mut session.replication, &player.profile.inventory, fresh);
        let anchor = player.view_anchor();
        let (biome, band) = generator.location_names(anchor.x, anchor.y);
        let current_self = self_state(player, biome, band);
        let self_state = if session.replication.last_self.as_ref() != Some(&current_self) {
            session.replication.last_self = Some(current_self.clone());
            Some(current_self)
        } else {
            None
        };

        session.replication.fresh = false;
        session.send(&ServerMessage::TickFrame(Box::new(TickFrame {
            tick,
            world_age: clock.age,
            chunks,
            players: public_players,
            inventory: inventory.slots,
            cursor: inventory.cursor,
            trash: inventory.trash,
            self_state,
            particles: in_interest,
            debug,
        })));
    }

    stats.players = players.len();
    (stats.awake_chunks, stats.awake_cells) = sim.awake_counts();
    stats.loaded_chunks = sim.chunk_count();
    (stats.loaded_regions, stats.dirty_regions) = regions.counts();
    stats.replicated_bytes = sessions
        .entries
        .values()
        .map(|session| session.replication.sent_bytes)
        .sum();
    for session in sessions.entries.values_mut() {
        session.replication.sent_bytes = 0;
    }
}

fn self_state(player: &crate::player::Player, biome: &str, band: &str) -> SelfState {
    let life = match &player.life {
        PlayerLife::Entering(_) => SelfLife::Entering,
        PlayerLife::Alive(avatar) => {
            let interaction = avatar.dig.interaction.unwrap_or(InteractionState {
                target: avatar.actor.cell(),
                status: InteractionStatus::None,
                progress: 0.0,
                dig_material: None,
            });
            SelfLife::Alive(SelfAvatarState {
                hp: avatar.health.hp,
                air: avatar.air,
                interaction,
            })
        }
        PlayerLife::Dead(_) => SelfLife::Dead,
        PlayerLife::Reviving(_) => SelfLife::Reviving,
    };
    SelfState {
        life,
        anchor: (!player.is_alive()).then(|| player.view_anchor()),
        mode: player.profile.mode,
        biome: biome.into(),
        band: band.into(),
    }
}

struct InventoryDelta {
    slots: Vec<(u16, Option<ItemStack>)>,
    cursor: Option<Option<ItemStack>>,
    trash: Option<Option<ItemStack>>,
}

fn inventory_delta(
    replication: &mut SessionReplication,
    inventory: &Inventory,
    fresh: bool,
) -> InventoryDelta {
    if fresh {
        replication.last_inventory = inventory.inner.slots.clone();
        replication.last_cursor = inventory.cursor;
        replication.last_trash = inventory.trash;
        return InventoryDelta {
            slots: inventory
                .inner
                .slots
                .iter()
                .enumerate()
                .map(|(index, stack)| (index as u16, *stack))
                .collect(),
            cursor: Some(inventory.cursor),
            trash: Some(inventory.trash),
        };
    }
    let slots = inventory
        .inner
        .slots
        .iter()
        .enumerate()
        .filter_map(|(index, stack)| {
            (replication.last_inventory.get(index) != Some(stack)).then_some((index as u16, *stack))
        })
        .collect();
    let cursor = (replication.last_cursor != inventory.cursor).then_some(inventory.cursor);
    let trash = (replication.last_trash != inventory.trash).then_some(inventory.trash);
    replication.last_inventory = inventory.inner.slots.clone();
    replication.last_cursor = inventory.cursor;
    replication.last_trash = inventory.trash;
    InventoryDelta {
        slots,
        cursor,
        trash,
    }
}

fn particles_in_interest(particles: &[ParticleSpawn], center: ChunkPos) -> Vec<ParticleSpawn> {
    let size = CHUNK_SIZE as f32;
    let min_x = (center.x - INTEREST_RADIUS_X) as f32 * size;
    let max_x = (center.x + INTEREST_RADIUS_X + 1) as f32 * size;
    let min_y = (center.y - INTEREST_RADIUS_Y) as f32 * size;
    let max_y = (center.y + INTEREST_RADIUS_Y + 1) as f32 * size;
    particles
        .iter()
        .filter(|p| p.x >= min_x && p.x < max_x && p.y >= min_y && p.y < max_y)
        .copied()
        .collect()
}

fn build_tiles(
    known: &mut FxHashSet<ChunkPos>,
    debug: bool,
    sim: &CellWorld,
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
