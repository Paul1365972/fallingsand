use crate::dig::DigState;
use crate::inventory::Inventory;
use crate::{MAX_AIR_SECONDS, MAX_HEALTH};
use fallingsand_core::{CellPos, Subcell};
use fallingsand_protocol::{GameMode, InputState, PlayerId, PlayerUuid, SlotAction, UseButton};
use fallingsand_sim::PlayerStamp;
use fallingsand_sim::physics::{Actor, Controller};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

pub const PLAYER_HALF_W: Subcell =
    Subcell::from_cells(fallingsand_sim::player::PLAYER_COLS as f32 * 0.5);
pub const PLAYER_HALF_H: Subcell =
    Subcell::from_cells(fallingsand_sim::player::STAND_ROWS as f32 * 0.5);
pub const PLAYER_MASS: f32 = 4.0 * PLAYER_HALF_W.to_cells() * PLAYER_HALF_H.to_cells();

#[derive(Default)]
pub struct Players {
    by_id: BTreeMap<PlayerId, Player>,
    by_uuid: FxHashMap<PlayerUuid, PlayerId>,
    next_id: u32,
}

impl Players {
    pub fn allocate_id(&mut self) -> Option<PlayerId> {
        let id = PlayerId(self.next_id);
        self.next_id = self.next_id.checked_add(1)?;
        Some(id)
    }

    pub fn insert(&mut self, player: Player) {
        let old_uuid = self.by_uuid.insert(player.uuid, player.id);
        let old_player = self.by_id.insert(player.id, player);
        debug_assert!(old_uuid.is_none());
        debug_assert!(old_player.is_none());
    }

    pub fn remove(&mut self, id: PlayerId) -> Option<Player> {
        let player = self.by_id.remove(&id)?;
        self.by_uuid.remove(&player.uuid);
        Some(player)
    }

    pub fn id_for_uuid(&self, uuid: PlayerUuid) -> Option<PlayerId> {
        self.by_uuid.get(&uuid).copied()
    }

    pub fn get(&self, id: PlayerId) -> Option<&Player> {
        self.by_id.get(&id)
    }

    pub fn get_mut(&mut self, id: PlayerId) -> Option<&mut Player> {
        self.by_id.get_mut(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PlayerId, &Player)> {
        self.by_id.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&PlayerId, &mut Player)> {
        self.by_id.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }
}

pub struct Player {
    pub id: PlayerId,
    pub uuid: PlayerUuid,
    pub name: String,
    pub profile: PlayerProfile,
    pub control: PlayerControl,
    pub life: PlayerLife,
}

pub struct PlayerProfile {
    pub mode: GameMode,
    pub selected_slot: u8,
    pub inventory: Inventory,
    pub history: Vec<String>,
}

pub struct PlayerControl {
    pub input: InputState,
    pub jump_pressed: bool,
    pub pending_commands: Vec<String>,
    pub pending_slot_actions: Vec<SlotAction>,
    pub pending_uses: Vec<(UseButton, CellPos)>,
    pub revive_requested: bool,
    pub last_input_tick: u64,
    pub last_chat_tick: u64,
}

impl PlayerControl {
    pub fn new(tick: u64) -> Self {
        Self {
            input: InputState::default(),
            jump_pressed: false,
            pending_commands: Vec::new(),
            pending_slot_actions: Vec::new(),
            pending_uses: Vec::new(),
            revive_requested: false,
            last_input_tick: tick,
            last_chat_tick: 0,
        }
    }

    pub fn reset_transient(&mut self, tick: u64) {
        self.input = InputState::default();
        self.jump_pressed = false;
        self.pending_commands.clear();
        self.pending_slot_actions.clear();
        self.pending_uses.clear();
        self.revive_requested = false;
        self.last_input_tick = tick;
    }
}

pub enum PlayerLife {
    Entering(EnteringPlayer),
    Alive(Avatar),
    Dead(DeadPlayer),
    Reviving(RevivingPlayer),
}

pub struct EnteringPlayer {
    pub materialization: Materialization,
}

#[derive(Clone, Copy)]
pub struct DeadPlayer {
    pub view_anchor: CellPos,
}

pub struct RevivingPlayer {
    pub death: DeadPlayer,
    pub materialization: Materialization,
}

pub struct Materialization {
    pub template: AvatarSnapshot,
    pub search: SpawnSearch,
}

impl Materialization {
    fn new(template: AvatarSnapshot) -> Self {
        let search = SpawnSearch::new(template.cell());
        Self { template, search }
    }

    fn fresh(spawn: CellPos) -> Self {
        Self::new(AvatarSnapshot::fresh(spawn))
    }
}

impl PlayerLife {
    pub fn materialization(&self) -> Option<&Materialization> {
        match self {
            PlayerLife::Entering(entering) => Some(&entering.materialization),
            PlayerLife::Reviving(reviving) => Some(&reviving.materialization),
            PlayerLife::Alive(_) | PlayerLife::Dead(_) => None,
        }
    }

    pub fn materialization_mut(&mut self) -> Option<&mut Materialization> {
        match self {
            PlayerLife::Entering(entering) => Some(&mut entering.materialization),
            PlayerLife::Reviving(reviving) => Some(&mut reviving.materialization),
            PlayerLife::Alive(_) | PlayerLife::Dead(_) => None,
        }
    }
}

pub struct Avatar {
    pub actor: Actor,
    pub stamp: PlayerStamp,
    pub controller: Controller,
    pub health: Health,
    pub air: f32,
    pub burning_secs: f32,
    pub flying: bool,
    pub dig: DigState,
    pub pending_impulse: (f32, f32),
    pub pending_crush_dv: f32,
}

pub struct Health {
    pub hp: f32,
    pub regen_delay_ticks: u64,
}

pub struct RestoredPlayer {
    pub mode: GameMode,
    pub selected_slot: u8,
    pub inventory: Inventory,
    pub history: Vec<String>,
    pub resume: ResumeSnapshot,
}

pub enum ResumeSnapshot {
    Alive(AvatarSnapshot),
    Dead { view_anchor: CellPos },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AvatarSnapshot {
    pub x: Subcell,
    pub y: Subcell,
    pub vx: Subcell,
    pub vy: Subcell,
    pub hp: f32,
    pub regen_delay_ticks: u64,
    pub air: f32,
    pub burning: f32,
    pub flying: bool,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            hp: MAX_HEALTH,
            regen_delay_ticks: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SearchWindow {
    pub min: CellPos,
    pub max: CellPos,
}

pub struct SpawnSearch {
    pub origin: CellPos,
    radius: i32,
    dy: i32,
    right: bool,
    window: SearchWindow,
}

impl SpawnSearch {
    pub fn new(origin: CellPos) -> Self {
        let window = SearchWindow::around(origin);
        Self {
            origin,
            radius: 0,
            dy: 0,
            right: false,
            window,
        }
    }

    pub fn candidate(&self) -> Option<CellPos> {
        let dx = self.radius.checked_sub(self.dy.checked_abs()?)?;
        let signed_dx = if self.right { dx } else { -dx };
        Some(CellPos::new(
            self.origin.x.checked_add(signed_dx)?,
            self.origin.y.checked_add(self.dy)?,
        ))
    }

    pub fn window(&self) -> SearchWindow {
        self.window
    }

    pub fn advance(&mut self) -> bool {
        let Some(abs_dy) = self.dy.checked_abs() else {
            return false;
        };
        let dx = self.radius - abs_dy;
        if dx != 0 && !self.right {
            self.right = true;
        } else if self.dy > -self.radius {
            self.dy -= 1;
            self.right = false;
        } else {
            let Some(radius) = self.radius.checked_add(1) else {
                return false;
            };
            self.radius = radius;
            self.dy = radius;
            self.right = false;
        }
        self.candidate().is_some()
    }

    pub fn center_window(&mut self, center: CellPos) {
        self.window = SearchWindow::around(center);
    }
}

impl SearchWindow {
    const SIZE: i32 = fallingsand_core::CHUNK_SIZE as i32;

    fn around(center: CellPos) -> Self {
        let low = Self::SIZE / 2;
        let high = Self::SIZE - low - 1;
        Self {
            min: CellPos::new(center.x.saturating_sub(low), center.y.saturating_sub(low)),
            max: CellPos::new(center.x.saturating_add(high), center.y.saturating_add(high)),
        }
    }

    pub fn contains(self, pos: CellPos) -> bool {
        pos.x >= self.min.x && pos.x <= self.max.x && pos.y >= self.min.y && pos.y <= self.max.y
    }
}

impl Player {
    pub fn new(
        id: PlayerId,
        uuid: PlayerUuid,
        name: String,
        tick: u64,
        restored: Option<RestoredPlayer>,
        spawn: CellPos,
    ) -> Self {
        let (profile, resume) = match restored {
            Some(restored) => (
                PlayerProfile {
                    mode: restored.mode,
                    selected_slot: restored.selected_slot,
                    inventory: restored.inventory,
                    history: restored.history,
                },
                Some(restored.resume),
            ),
            None => (
                PlayerProfile {
                    mode: GameMode::default(),
                    selected_slot: 0,
                    inventory: Inventory::default(),
                    history: Vec::new(),
                },
                None,
            ),
        };
        let life = match resume {
            Some(ResumeSnapshot::Dead { view_anchor }) => {
                PlayerLife::Dead(DeadPlayer { view_anchor })
            }
            Some(ResumeSnapshot::Alive(template)) => {
                let template = template.sanitized();
                PlayerLife::Entering(EnteringPlayer {
                    materialization: Materialization::new(template),
                })
            }
            None => PlayerLife::Entering(EnteringPlayer {
                materialization: Materialization::fresh(spawn),
            }),
        };
        Self {
            id,
            uuid,
            name,
            profile,
            control: PlayerControl::new(tick),
            life,
        }
    }

    pub fn is_alive(&self) -> bool {
        matches!(self.life, PlayerLife::Alive(_))
    }

    pub fn avatar(&self) -> Option<&Avatar> {
        match &self.life {
            PlayerLife::Alive(avatar) => Some(avatar),
            _ => None,
        }
    }

    pub fn avatar_mut(&mut self) -> Option<&mut Avatar> {
        match &mut self.life {
            PlayerLife::Alive(avatar) => Some(avatar),
            _ => None,
        }
    }

    pub fn view_anchor(&self) -> CellPos {
        match &self.life {
            PlayerLife::Entering(entering) => entering.materialization.search.origin,
            PlayerLife::Alive(avatar) => avatar.actor.cell(),
            PlayerLife::Dead(dead) => dead.view_anchor,
            PlayerLife::Reviving(reviving) => reviving.death.view_anchor,
        }
    }

    pub fn begin_revive(&mut self, spawn: CellPos, tick: u64) -> bool {
        let PlayerLife::Dead(death) = &self.life else {
            return false;
        };
        let death = *death;
        self.transition_life(
            PlayerLife::Reviving(RevivingPlayer {
                death,
                materialization: Materialization::fresh(spawn),
            }),
            tick,
        );
        true
    }

    pub fn die(&mut self, view_anchor: CellPos, tick: u64) {
        self.transition_life(PlayerLife::Dead(DeadPlayer { view_anchor }), tick);
    }

    pub fn finish_materialization(&mut self, avatar: Avatar, tick: u64) {
        self.transition_life(PlayerLife::Alive(avatar), tick);
    }

    fn transition_life(&mut self, life: PlayerLife, tick: u64) {
        self.life = life;
        self.control.reset_transient(tick);
    }
}

impl AvatarSnapshot {
    pub fn sanitized(&self) -> Self {
        let mut record = self.clone();
        record.hp = if record.hp.is_finite() {
            record.hp
        } else {
            MAX_HEALTH
        }
        .clamp(0.0, MAX_HEALTH);
        record.air = if record.air.is_finite() {
            record.air
        } else {
            MAX_AIR_SECONDS
        }
        .clamp(0.0, MAX_AIR_SECONDS);
        record.burning = if record.burning.is_finite() {
            record.burning
        } else {
            0.0
        }
        .max(0.0);
        record
    }

    pub fn fresh(spawn: CellPos) -> Self {
        Self {
            x: Subcell::from_cell(spawn.x),
            y: Subcell::from_cell(spawn.y),
            vx: Subcell::ZERO,
            vy: Subcell::ZERO,
            hp: MAX_HEALTH,
            regen_delay_ticks: 0,
            air: MAX_AIR_SECONDS,
            burning: 0.0,
            flying: false,
        }
    }

    pub fn cell(&self) -> CellPos {
        CellPos::new(self.x.floor_cell(), self.y.floor_cell())
    }

    pub fn from_avatar(avatar: &Avatar) -> Self {
        Self {
            x: avatar.actor.x,
            y: avatar.actor.y
                + Subcell::from_cells(
                    (fallingsand_sim::player::STAND_ROWS as i32 / 2 - avatar.actor.rows() / 2)
                        as f32,
                ),
            vx: avatar.actor.vx,
            vy: avatar.actor.vy,
            hp: avatar.health.hp,
            regen_delay_ticks: avatar.health.regen_delay_ticks,
            air: avatar.air,
            burning: avatar.burning_secs,
            flying: avatar.flying,
        }
    }
}
