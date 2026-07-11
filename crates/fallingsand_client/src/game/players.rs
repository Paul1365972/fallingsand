use super::Changes;
use bevy::math::Vec2;
use fallingsand_protocol::{GameMode, PlayerId, TickFrame};
use std::collections::HashMap;

const DAMAGE_FLASH_SECS: f32 = 0.35;

pub struct RemotePlayer {
    pub pos: Vec2,
    pub height: u8,
    pub burning: bool,
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
        if let Some(self_state) = tick.self_state {
            if self_state.hp < you.hp - 0.01 && self_state.hp > 0.0 {
                you.damage_flash = DAMAGE_FLASH_SECS;
            }
            if you.mode != self_state.mode {
                changes.mode = true;
            }
            you.hp = self_state.hp;
            you.air = self_state.air;
            you.mode = self_state.mode;
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct SelfState {
    pub present: bool,
    pub pos: Vec2,
    pub hp: f32,
    pub air: f32,
    pub burning: bool,
    pub mode: GameMode,
    pub damage_flash: f32,
}
