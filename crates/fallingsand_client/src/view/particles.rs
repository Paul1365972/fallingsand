use super::Game;
use bevy::prelude::*;
use fallingsand_protocol::InteractionStatus;

const SPRAY_TTL: f32 = 0.5;
const SPRAY_GRAVITY: f32 = 260.0;

pub struct ParticleVisual {
    pub position: Vec2,
    velocity: Vec2,
    color: Vec3,
    ttl: f32,
    max_ttl: f32,
}

#[derive(Clone, Copy)]
pub struct WorldQuad {
    pub center: Vec2,
    pub size: Vec2,
    pub color: Vec4,
}

#[derive(Resource, Default)]
pub struct ParticleVisuals {
    particles: Vec<ParticleVisual>,
    pub quads: Vec<WorldQuad>,
}

impl ParticleVisuals {
    pub fn len(&self) -> usize {
        self.particles.len()
    }
}

pub fn update_particles(
    mut game: ResMut<Game>,
    time: Res<Time>,
    mut visuals: ResMut<ParticleVisuals>,
) {
    let dt = time.delta_secs();
    if let Some(ingame) = game.0.ingame_mut() {
        visuals
            .particles
            .extend(ingame.particles.drain(..).map(|spawn| {
                ParticleVisual {
                    position: Vec2::new(spawn.x, spawn.y),
                    velocity: Vec2::new(spawn.vx, spawn.vy),
                    color: Color::srgb_u8(spawn.color[0], spawn.color[1], spawn.color[2])
                        .to_linear()
                        .to_vec3(),
                    ttl: SPRAY_TTL,
                    max_ttl: SPRAY_TTL,
                }
            }));
    } else {
        visuals.particles.clear();
    }

    let mut index = 0;
    while index < visuals.particles.len() {
        let particle = &mut visuals.particles[index];
        particle.ttl -= dt;
        if particle.ttl <= 0.0 {
            visuals.particles.swap_remove(index);
            continue;
        }
        particle.velocity.y -= SPRAY_GRAVITY * dt;
        particle.position += particle.velocity * dt;
        index += 1;
    }

    let highlight = game
        .0
        .playing()
        .and_then(|ingame| ingame.you.life.avatar().map(|avatar| avatar.interaction))
        .and_then(|state| status_color(state.status, state.progress).map(|color| (state, color)));
    visuals.quads.clear();
    let ParticleVisuals { particles, quads } = &mut *visuals;
    quads.extend(particles.iter().map(|particle| {
        WorldQuad {
            center: particle.position,
            size: Vec2::ONE,
            color: particle
                .color
                .extend((particle.ttl / particle.max_ttl).clamp(0.0, 1.0)),
        }
    }));
    if let Some((state, color)) = highlight {
        visuals.quads.push(WorldQuad {
            center: Vec2::new(state.target.x as f32 + 0.5, state.target.y as f32 + 0.5),
            size: Vec2::ONE,
            color: color.to_linear().to_f32_array().into(),
        });
    }
}

fn status_color(status: InteractionStatus, progress: f32) -> Option<Color> {
    Some(match status {
        InteractionStatus::Valid => Color::srgba(0.3, 1.0, 0.4, 0.32 + progress * 0.35),
        InteractionStatus::OutOfReach | InteractionStatus::NoTarget => {
            Color::srgba(0.55, 0.55, 0.6, 0.32)
        }
        InteractionStatus::Occupied | InteractionStatus::Undiggable => {
            Color::srgba(1.0, 0.25, 0.2, 0.45)
        }
        InteractionStatus::WrongTool | InteractionStatus::NotPlaceable => {
            Color::srgba(1.0, 0.55, 0.12, 0.48)
        }
        InteractionStatus::InventoryFull => Color::srgba(0.75, 0.2, 1.0, 0.5),
        InteractionStatus::None => return None,
    })
}
