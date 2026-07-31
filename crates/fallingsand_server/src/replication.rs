use crate::inventory::Inventory;
use crate::player::{PlayerLife, Players};
use crate::regions::RegionMap;
use crate::session::Sessions;
use crate::{INTEREST_RADIUS_X, INTEREST_RADIUS_Y};
use fallingsand_core::FogMask;
use fallingsand_core::{
    CHUNK_AREA, CHUNK_SIZE, Calendar, CellOffset, CellPos, ChunkPos, ItemStack, Motion,
};
use fallingsand_protocol::{
    ChunkDebugRects, ChunkOp, DebugBody, DebugMotion, InteractionState, InteractionStatus,
    ParticleSpawn, PlayerAvatarState, PlayerId, PlayerState, SelfAvatarState, SelfLife, SelfState,
    ServerMessage, TickFrame, cells_to_wire,
};
use fallingsand_sim::CellWorld;
use fallingsand_sim::body::Bodies;
use fallingsand_worldgen::WorldGenerator;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;

pub struct SessionReplication {
    pub known_chunks: FxHashMap<ChunkPos, FogMask>,
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
            known_chunks: FxHashMap::default(),
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

pub struct ReplicationMetrics {
    pub players: usize,
    pub awake_chunks: usize,
    pub awake_cells: u64,
    pub loaded_chunks: usize,
    pub loaded_regions: u32,
    pub replicated_bytes: u64,
    pub replicated_cells: u64,
    pub written_cells: u64,
    pub visible_cells: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn replicate(
    sessions: &mut Sessions,
    players: &Players,
    sim: &CellWorld,
    bodies: &Bodies,
    clock: &Calendar,
    regions: &RegionMap,
    generator: &WorldGenerator,
    particles: &[ParticleSpawn],
    replication: &mut ReplicationState,
) -> ReplicationMetrics {
    let tick = sim.tick();
    let all_players: Vec<PlayerState> = players
        .iter()
        .map(|(&id, player)| PlayerState {
            player: id,
            avatar: player.avatar().map(|avatar| {
                let anchor = bodies
                    .cell(avatar.body_id)
                    .expect("an alive avatar owns its body");
                PlayerAvatarState {
                    cx: anchor.x,
                    cy: anchor.y,
                    height: avatar.controller.rows as u8,
                    burning: avatar.burning_secs > 0.0,
                }
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

    let mut replicated_cells = 0;
    for session in sessions.active_iter_mut() {
        let Some(player_id) = session.player() else {
            continue;
        };
        let Some(player) = players.get(player_id) else {
            continue;
        };
        let center = player.view_anchor(bodies).chunk();
        let mut interest = FxHashSet::default();
        for dy in -INTEREST_RADIUS_Y..=INTEREST_RADIUS_Y {
            for dx in -INTEREST_RADIUS_X..=INTEREST_RADIUS_X {
                let pos = center.translated(dx, dy);
                if sim.chunk(pos).is_some() {
                    interest.insert(pos);
                }
            }
        }

        let mut debug_rects = Vec::new();
        let chunks = build_tiles(
            &mut session.replication.known_chunks,
            session.replication.debug,
            sim,
            &interest,
            &mut debug_rects,
            &mut replicated_cells,
        );
        let (debug_bodies, debug_motion) = if session.replication.debug {
            debug_payload(sim, bodies, &interest)
        } else {
            (Vec::new(), Vec::new())
        };
        let in_interest = particles_in_interest(particles, center);
        let public_players = if session.replication.fresh {
            all_players.clone()
        } else {
            changed_players.clone()
        };
        let fresh = session.replication.fresh;
        let inventory = inventory_delta(&mut session.replication, &player.profile.inventory, fresh);
        let anchor = player.view_anchor(bodies);
        let (biome, locale) = generator.location_names(anchor.x, anchor.y);
        let current_self = self_state(player, bodies, anchor, biome, locale);
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
            debug_rects,
            debug_bodies,
            debug_motion,
        })));
    }

    let (awake_chunks, awake_cells) = sim.awake_counts();
    let changes = sim.change_counts();
    let replicated_bytes = sessions
        .entries
        .values()
        .map(|session| session.replication.sent_bytes)
        .sum();
    for session in sessions.entries.values_mut() {
        session.replication.sent_bytes = 0;
    }
    ReplicationMetrics {
        players: players.len(),
        awake_chunks,
        awake_cells,
        loaded_chunks: sim.chunk_count(),
        loaded_regions: regions.len() as u32,
        replicated_bytes,
        replicated_cells,
        written_cells: changes.writes as u64,
        visible_cells: changes.visible as u64,
    }
}

fn self_state(
    player: &crate::player::Player,
    bodies: &Bodies,
    anchor: CellPos,
    biome: &str,
    locale: &str,
) -> SelfState {
    let life = match &player.life {
        PlayerLife::Entering(_) => SelfLife::Entering,
        PlayerLife::Alive(avatar) => {
            let interaction = avatar.dig.interaction.unwrap_or(InteractionState {
                target: anchor,
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
        anchor: (!player.is_alive()).then(|| player.view_anchor(bodies)),
        mode: player.profile.mode,
        biome: biome.into(),
        locale: locale.into(),
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

const MAX_DEBUG_MOTION: usize = 8192;

fn debug_payload(
    sim: &CellWorld,
    bodies: &Bodies,
    interest: &FxHashSet<ChunkPos>,
) -> (Vec<DebugBody>, Vec<DebugMotion>) {
    let mut debug_bodies: Vec<DebugBody> = bodies
        .rasters()
        .filter(|(_, raster)| raster.iter().any(|pos| interest.contains(&pos.chunk())))
        .map(|(id, raster)| DebugBody {
            id,
            cells: raster.to_vec(),
        })
        .collect();
    debug_bodies.sort_unstable_by_key(|body| body.id);

    let mut motion = Vec::new();
    let mut chunks: Vec<ChunkPos> = interest.iter().copied().collect();
    chunks.sort_unstable_by_key(|pos| (pos.y, pos.x));
    'gather: for pos in chunks {
        let chunk = sim.chunk(pos).expect("interest chunks are loaded");
        let rect = chunk.sim_rect();
        if rect.is_empty() {
            continue;
        }
        let base = pos.base_cell();
        for y in rect.min_y..=rect.max_y {
            for x in rect.min_x..=rect.max_x {
                let cell = chunk.get(CellOffset::new(x, y));
                let Motion::Velocity(vx, vy) = cell.motion() else {
                    continue;
                };
                if (vx, vy) == (0, 0) && !cell.is_stressed() {
                    continue;
                }
                motion.push(DebugMotion {
                    pos: CellPos::new(base.x + i32::from(x), base.y + i32::from(y)),
                    vx: vx as i16,
                    vy: vy as i16,
                    stressed: cell.is_stressed(),
                });
                if motion.len() >= MAX_DEBUG_MOTION {
                    break 'gather;
                }
            }
        }
    }
    (debug_bodies, motion)
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
    known: &mut FxHashMap<ChunkPos, FogMask>,
    debug: bool,
    sim: &CellWorld,
    interest: &FxHashSet<ChunkPos>,
    debug_rects: &mut Vec<ChunkDebugRects>,
    replicated_cells: &mut u64,
) -> Vec<ChunkOp> {
    let mut ops = Vec::new();
    known.retain(|&pos, _| {
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
        let fog = *chunk.fog();
        match known.insert(pos, fog) {
            None => {
                *replicated_cells += CHUNK_AREA as u64;
                ops.push(ChunkOp::Load {
                    pos,
                    cells: cells_to_wire(chunk.cells()),
                    fog,
                });
                continue;
            }
            Some(sent) if sent != fog => ops.push(ChunkOp::Fog { pos, fog }),
            Some(_) => {}
        }
        let rect = chunk.change_rect();
        if rect.is_empty() {
            continue;
        }
        *replicated_cells += u64::from(rect.width() * rect.height());
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
