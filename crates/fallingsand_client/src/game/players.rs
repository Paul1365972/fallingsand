use super::Changes;
use bevy::math::Vec2;
use fallingsand_protocol::{
    GameMode, InteractionState, InteractionStatus, LifeState, PlayerId, TickFrame,
};
use std::collections::HashMap;

const DAMAGE_FLASH_SECS: f32 = 0.35;

pub struct RemotePlayer {
    pub pos: Vec2,
    pub height: u8,
    pub burning: bool,
}

impl RemotePlayer {
    fn feet_y(&self) -> f32 {
        self.pos.y - 0.5 - (self.height as i32 / 2) as f32
    }

    pub fn top_y(&self) -> f32 {
        self.feet_y() + self.height.max(1) as f32
    }

    pub fn center(&self) -> Vec2 {
        Vec2::new(self.pos.x, self.feet_y() + self.height.max(1) as f32 * 0.5)
    }
}

#[derive(Default)]
pub struct Players {
    pub roster: HashMap<PlayerId, RemotePlayer>,
    pub names: HashMap<PlayerId, String>,
}

impl Players {
    pub fn clear(&mut self) {
        self.roster.clear();
        self.names.clear();
    }

    pub(super) fn apply(
        &mut self,
        tick: &TickFrame,
        local: Option<PlayerId>,
        you: &mut SelfState,
        changes: &mut Changes,
    ) {
        for state in &tick.players {
            if state.life == LifeState::Dead {
                self.roster.remove(&state.player);
                if local == Some(state.player) {
                    you.present = false;
                    you.burning = false;
                }
                continue;
            }
            let pos = Vec2::new(state.cx as f32 + 0.5, state.cy as f32 + 0.5);
            if local == Some(state.player) {
                you.pos = pos;
                you.burning = state.burning;
                you.present = true;
            }
            self.roster
                .entry(state.player)
                .and_modify(|player| {
                    player.pos = pos;
                    player.height = state.height;
                    player.burning = state.burning;
                })
                .or_insert(RemotePlayer {
                    pos,
                    height: state.height,
                    burning: state.burning,
                });
        }
        if let Some(self_state) = &tick.self_state {
            if self_state.hp < you.hp - 0.01 && self_state.hp > 0.0 {
                you.damage_flash = DAMAGE_FLASH_SECS;
            }
            if you.mode != self_state.mode {
                changes.mode = true;
            }
            you.hp = self_state.hp;
            you.life = self_state.life;
            you.present = self_state.life == LifeState::Alive;
            you.air = self_state.air;
            you.mode = self_state.mode;
            you.biome = self_state.biome.clone();
            you.band = self_state.band.clone();
            you.interaction = self_state.interaction;
        }
    }
}

#[derive(Clone)]
pub struct SelfState {
    pub present: bool,
    pub pos: Vec2,
    pub hp: f32,
    pub life: LifeState,
    pub air: f32,
    pub burning: bool,
    pub mode: GameMode,
    pub damage_flash: f32,
    pub biome: String,
    pub band: String,
    pub interaction: InteractionState,
}

impl Default for SelfState {
    fn default() -> Self {
        Self {
            present: false,
            pos: Vec2::ZERO,
            hp: 0.0,
            life: LifeState::Alive,
            air: 0.0,
            burning: false,
            mode: GameMode::default(),
            damage_flash: 0.0,
            biome: String::new(),
            band: String::new(),
            interaction: InteractionState {
                target: fallingsand_core::CellPos::new(0, 0),
                status: InteractionStatus::None,
                progress: 0.0,
            },
        }
    }
}
