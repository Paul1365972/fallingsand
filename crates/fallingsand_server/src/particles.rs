use crate::player::{PlayerLife, Players};
use fallingsand_core::content;
use fallingsand_core::{CellPos, TICK_DT};
use fallingsand_protocol::{InteractionStatus, ParticleSpawn, PlayerId};
use fallingsand_rng::{Hash, Rng};
use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

const SPRAY_INTERVAL: f32 = 1.0 / 12.0;
const MAX_PER_TICK: usize = 256;

#[derive(Default)]
pub struct ParticleEmitter {
    spray_accum: BTreeMap<PlayerId, f32>,
    pub spawns: Vec<ParticleSpawn>,
}

impl ParticleEmitter {
    pub fn emit(&mut self, players: &Players, tick: u64) {
        self.spawns.clear();
        self.emit_spray(players, tick);
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
    }
}
