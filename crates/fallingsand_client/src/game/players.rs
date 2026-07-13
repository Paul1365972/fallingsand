use super::Changes;
use bevy::math::Vec2;
use fallingsand_protocol::{GameMode, PlayerId, SelfLife, TickFrame};
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
    pub avatars: HashMap<PlayerId, RemotePlayer>,
    pub names: HashMap<PlayerId, String>,
}

impl Players {
    pub fn clear(&mut self) {
        self.avatars.clear();
        self.names.clear();
    }

    pub(super) fn apply(&mut self, tick: &TickFrame, you: &mut SelfState, changes: &mut Changes) {
        for state in &tick.players {
            let Some(avatar) = state.avatar else {
                self.avatars.remove(&state.player);
                continue;
            };
            let pos = Vec2::new(avatar.cx as f32 + 0.5, avatar.cy as f32 + 0.5);
            self.avatars
                .entry(state.player)
                .and_modify(|player| {
                    player.pos = pos;
                    player.height = avatar.height;
                    player.burning = avatar.burning;
                })
                .or_insert(RemotePlayer {
                    pos,
                    height: avatar.height,
                    burning: avatar.burning,
                });
        }
        if let Some(self_state) = &tick.self_state {
            let old_hp = you.life.avatar().map_or(0.0, |avatar| avatar.hp);
            let hp = self_state.life.avatar().map_or(0.0, |avatar| avatar.hp);
            if hp < old_hp - 0.01 && hp > 0.0 {
                you.damage_flash = DAMAGE_FLASH_SECS;
            }
            if you.mode != self_state.mode {
                changes.mode = true;
            }
            you.life = self_state.life;
            you.mode = self_state.mode;
            you.biome = self_state.biome.clone();
            you.band = self_state.band.clone();
        }
    }
}

#[derive(Clone)]
pub struct SelfState {
    pub life: SelfLife,
    pub mode: GameMode,
    pub damage_flash: f32,
    pub biome: String,
    pub band: String,
}

impl Default for SelfState {
    fn default() -> Self {
        Self {
            life: SelfLife::Entering,
            mode: GameMode::default(),
            damage_flash: 0.0,
            biome: String::new(),
            band: String::new(),
        }
    }
}
