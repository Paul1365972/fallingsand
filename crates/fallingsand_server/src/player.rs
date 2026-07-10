use crate::inventory::Inventory;
use crate::persistence::{PlayerRecord, slots_to_record, stack_to_record};
use crate::{MAX_AIR_SECS, MAX_HP};
use bevy_ecs::prelude::*;
use fallingsand_core::{Fixed, ItemRegistry};
use fallingsand_protocol::{GameMode, InputState, PlayerId, PlayerUuid};
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
    pub brush_radius: u8,
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

impl Default for Health {
    fn default() -> Self {
        Self {
            hp: MAX_HP,
            last_damage_tick: 0,
        }
    }
}

#[derive(Component, Default)]
pub struct DigState {
    pub budget: f32,
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

impl Burning {
    pub fn active(&self) -> bool {
        self.secs > 0.0
    }
}

#[allow(clippy::too_many_arguments)]
pub fn player_record(
    item_reg: &ItemRegistry,
    player: &Player,
    body: &Actor,
    health: &Health,
    mode: &Mode,
    air: &Air,
    burning: &Burning,
    inventory: &Inventory,
) -> PlayerRecord {
    PlayerRecord {
        x: body.x,
        y: body.y + (PLAYER_HALF_H - body.half_h),
        hp: health.hp,
        mode: mode.0,
        air: air.secs,
        burning: burning.secs,
        flying: player.flying,
        inventory: slots_to_record(item_reg, &inventory.inner),
        cursor: stack_to_record(item_reg, inventory.cursor),
        trash: stack_to_record(item_reg, inventory.trash),
    }
}
