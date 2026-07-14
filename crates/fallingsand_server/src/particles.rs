use crate::player::{PlayerLife, Players};
use fallingsand_core::content;
use fallingsand_core::{CellPos, TICK_DT};
use fallingsand_protocol::{InteractionStatus, ParticleKind, ParticleSpawn, PlayerId};
use fallingsand_rng::{Hash, Rng};
use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

const SPRAY_INTERVAL: f32 = 1.0 / 12.0;
const FLAME_INTERVAL: f32 = 0.05;
const FLAME_PER_BURST: usize = 2;
const PLAYER_WIDTH: f32 = fallingsand_sim::player::PLAYER_COLS as f32;
const MAX_PER_TICK: usize = 256;

#[derive(Default)]
pub struct ParticleEmitter {
    flame_accum: f32,
    spray_accum: BTreeMap<PlayerId, f32>,
    pub spawns: Vec<ParticleSpawn>,
}

impl ParticleEmitter {
    pub fn emit(&mut self, players: &Players, tick: u64) {
        self.spawns.clear();
        self.emit_spray(players, tick);
        self.emit_flame(players, tick);
        self.spawns.truncate(MAX_PER_TICK);
    }

    fn emit_spray(&mut self, players: &Players, tick: u64) {
        let mut digging = Vec::new();
        for (&id, player) in players.iter() {
            let PlayerLife::Alive(avatar) = &player.life else {
                continue;
            };
            let Some(interaction) = avatar.dig.interaction else {
                continue;
            };
            if interaction.status != InteractionStatus::Valid {
                continue;
            }
            let Some(material) = interaction.dig_material else {
                continue;
            };
            digging.push((id, interaction.target, material));
        }
        self.spray_accum
            .retain(|id, _| digging.iter().any(|(active, ..)| active == id));

        for (id, target, material) in digging {
            let accum = self.spray_accum.entry(id).or_default();
            *accum += TICK_DT;
            let mut count = 0;
            while *accum >= SPRAY_INTERVAL {
                *accum -= SPRAY_INTERVAL;
                let mut rng = Hash::seed(tick).add(id.0 as u64).add(count).rng();
                self.spawns.push(spray(target, material, &mut rng));
                count += 1;
            }
        }
    }

    fn emit_flame(&mut self, players: &Players, tick: u64) {
        self.flame_accum += TICK_DT;
        if self.flame_accum < FLAME_INTERVAL {
            return;
        }
        self.flame_accum -= FLAME_INTERVAL;
        for (&id, player) in players.iter() {
            let PlayerLife::Alive(avatar) = &player.life else {
                continue;
            };
            if avatar.burning_secs <= 0.0 {
                continue;
            }
            let cx = avatar.actor.x.to_f32();
            let cy = avatar.actor.y.to_f32();
            let height = avatar.actor.rows().max(1) as f32;
            for n in 0..FLAME_PER_BURST {
                let mut rng = Hash::seed(tick).add(id.0 as u64).add(n as u64).rng();
                self.spawns.push(flame(cx, cy, height, &mut rng));
            }
        }
    }
}

fn spray(target: CellPos, material: fallingsand_core::MaterialId, rng: &mut Rng) -> ParticleSpawn {
    let colors = content::material(material).colors;
    let rgba = colors[rng.draw().range(0, colors.len() as i32 - 1) as usize];
    let angle = FRAC_PI_4 + rng.draw().unit() * FRAC_PI_2;
    let speed = 25.0 + rng.draw().unit() * 55.0;
    let jx = rng.draw().unit() - 0.5;
    let jy = rng.draw().unit() - 0.5;
    ParticleSpawn {
        x: target.x as f32 + 0.5 + jx,
        y: target.y as f32 + 0.5 + jy,
        vx: angle.cos() * speed,
        vy: angle.sin() * speed,
        color: [rgba[0], rgba[1], rgba[2]],
        kind: ParticleKind::Spray,
    }
}

fn flame(cx: f32, cy: f32, height: f32, rng: &mut Rng) -> ParticleSpawn {
    let ox = (rng.draw().unit() - 0.5) * PLAYER_WIDTH;
    let oy = (rng.draw().unit() - 0.5) * height;
    let warm = (0.4 + rng.draw().unit() * 0.5).min(1.0);
    ParticleSpawn {
        x: cx + ox,
        y: cy + oy,
        vx: (rng.draw().unit() - 0.5) * 14.0,
        vy: 24.0 + rng.draw().unit() * 26.0,
        color: [255, (warm * 255.0) as u8, 25],
        kind: ParticleKind::Flame,
    }
}
