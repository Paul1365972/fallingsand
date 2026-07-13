use crate::dig::DigState;
use crate::inventory::Inventory;
use crate::persistence::{
    PlayerRecord, player_slots_from_record, slots_to_record, stack_from_record, stack_to_record,
};
use crate::{MAX_AIR_SECS, MAX_HP};
use bevy_ecs::prelude::*;
use fallingsand_core::{CellPos, Fixed, HOTBAR_SLOTS, ItemRegistry};
use fallingsand_protocol::{GameMode, InputState, LifeState, PlayerId, PlayerUuid};
use fallingsand_sim::PlayerStamp;
use fallingsand_sim::physics::{Actor, Controller};

pub const PLAYER_HALF_W: Fixed = Fixed::from_f32(fallingsand_sim::player::PLAYER_COLS as f32 * 0.5);
pub const PLAYER_HALF_H: Fixed = Fixed::from_f32(fallingsand_sim::player::STAND_ROWS as f32 * 0.5);
pub const PLAYER_MASS: f32 = 4.0 * PLAYER_HALF_W.to_f32() * PLAYER_HALF_H.to_f32();

#[derive(Component)]
pub struct Player {
    pub id: PlayerId,
    pub uuid: PlayerUuid,
    pub name: String,
    pub input: InputState,
    pub jump_pressed: bool,
    pub flying: bool,
    pub selected_slot: u8,
    pub session_generation: u64,
    pub revive_requested: bool,
    pub last_input_tick: u64,
}

#[derive(Component)]
pub struct PlayerActor(pub Actor);

#[derive(Component, Default)]
pub struct PlayerRaster(pub PlayerStamp);

#[derive(Component, Default)]
pub struct Control(pub Controller);

#[derive(Component)]
pub struct Health {
    pub hp: f32,
    pub last_damage_tick: u64,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct Life(pub LifeState);

impl Default for Health {
    fn default() -> Self {
        Self {
            hp: MAX_HP,
            last_damage_tick: 0,
        }
    }
}

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
pub struct Mode(pub GameMode);

#[derive(Component)]
pub struct Air {
    pub secs: f32,
}

impl Default for Air {
    fn default() -> Self {
        Self { secs: MAX_AIR_SECS }
    }
}

#[derive(Component, Default)]
pub struct Burning {
    pub secs: f32,
}

#[derive(Component, Default)]
pub struct ChatHistory(pub Vec<String>);

impl Burning {
    pub fn active(&self) -> bool {
        self.secs > 0.0
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_player(
    commands: &mut Commands,
    item_reg: &ItemRegistry,
    id: PlayerId,
    uuid: PlayerUuid,
    name: String,
    tick: u64,
    session_generation: u64,
    record: Option<&PlayerRecord>,
    spawn: CellPos,
) -> Entity {
    commands
        .spawn((
            Player {
                id,
                uuid,
                name,
                input: Default::default(),
                jump_pressed: false,
                flying: record.map(|r| r.flying).unwrap_or(false),
                selected_slot: record
                    .map(|r| r.selected.min(HOTBAR_SLOTS as u8 - 1))
                    .unwrap_or(0),
                session_generation,
                revive_requested: false,
                last_input_tick: tick,
            },
            PlayerActor({
                let mut actor = Actor::new(
                    record.map(|r| r.x).unwrap_or(Fixed::from_cell(spawn.x)),
                    record.map(|r| r.y).unwrap_or(Fixed::from_cell(spawn.y)),
                    PLAYER_HALF_W,
                    PLAYER_HALF_H,
                );
                if let Some(record) = record {
                    actor.vx = record.vx;
                    actor.vy = record.vy;
                }
                actor
            }),
            PlayerRaster::default(),
            Control::default(),
            Health {
                hp: record
                    .map(|r| r.hp)
                    .filter(|hp| hp.is_finite())
                    .unwrap_or(MAX_HP)
                    .clamp(0.0, MAX_HP),
                last_damage_tick: 0,
            },
            Life(record.map(|r| r.life).unwrap_or_default()),
            DigState::default(),
            Mode(record.map(|r| r.mode).unwrap_or_default()),
            Air {
                secs: record
                    .map(|r| r.air)
                    .filter(|air| air.is_finite())
                    .unwrap_or(MAX_AIR_SECS)
                    .clamp(0.0, MAX_AIR_SECS),
            },
            Burning {
                secs: record
                    .map(|r| r.burning)
                    .filter(|secs| secs.is_finite())
                    .unwrap_or(0.0)
                    .max(0.0),
            },
            ChatHistory(record.map(|r| r.history.clone()).unwrap_or_default()),
            Inventory::with(
                player_slots_from_record(
                    item_reg,
                    record.map(|r| r.inventory.as_slice()).unwrap_or(&[]),
                ),
                record.and_then(|r| stack_from_record(item_reg, &r.cursor)),
                record.and_then(|r| stack_from_record(item_reg, &r.trash)),
            ),
        ))
        .id()
}

#[allow(clippy::too_many_arguments)]
pub fn player_record(
    item_reg: &ItemRegistry,
    player: &Player,
    body: &Actor,
    health: &Health,
    life: &Life,
    mode: &Mode,
    air: &Air,
    burning: &Burning,
    inventory: &Inventory,
    history: &ChatHistory,
) -> PlayerRecord {
    PlayerRecord {
        x: body.x,
        y: body.y
            + Fixed::from_int(fallingsand_sim::player::STAND_ROWS as i32 / 2 - body.rows() / 2),
        hp: health.hp,
        life: life.0,
        vx: body.vx,
        vy: body.vy,
        mode: mode.0,
        air: air.secs,
        burning: burning.secs,
        flying: player.flying,
        selected: player.selected_slot,
        inventory: slots_to_record(item_reg, &inventory.inner),
        cursor: stack_to_record(item_reg, inventory.cursor),
        trash: stack_to_record(item_reg, inventory.trash),
        history: history.0.clone(),
    }
}
