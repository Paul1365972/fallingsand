use super::Game;
use super::camera::WORLD_LAYER;
use super::{PLAYER_DUCK_SIZE, PLAYER_SIZE};
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use fallingsand_core::{Phase, REACH, SURVIVAL_REACH};
use fallingsand_protocol::GameMode;
use fallingsand_rng::Rng;

const SPRAY_TTL: f32 = 0.5;
const SPRAY_GRAVITY: f32 = 260.0;
const SPRAY_PER_FRAME: usize = 3;
const FLAME_TTL: f32 = 0.45;
const FLAME_INTERVAL: f32 = 0.05;

#[derive(Component)]
pub struct Particle {
    velocity: Vec2,
    gravity: f32,
    ttl: f32,
    max_ttl: f32,
}

pub fn spawn_particles(
    mut commands: Commands,
    game: Res<Game>,
    time: Res<Time>,
    mut flame_accum: Local<f32>,
    mut rng: Local<Rng>,
) {
    let Some(ingame) = game.0.playing() else {
        return;
    };
    if ingame.paused() {
        return;
    }

    spawn_dig_spray(&mut commands, &game.0, ingame, &mut rng);

    *flame_accum += time.delta_secs();
    if *flame_accum < FLAME_INTERVAL {
        return;
    }
    *flame_accum = 0.0;
    for remote in ingame.players.roster.values() {
        if !remote.burning {
            continue;
        }
        let size = if remote.ducking {
            PLAYER_DUCK_SIZE
        } else {
            PLAYER_SIZE
        };
        for _ in 0..2 {
            let offset = Vec2::new(
                (rng.draw().unit() - 0.5) * size.x,
                (rng.draw().unit() - 0.5) * size.y,
            );
            let warm = rng.draw().unit();
            let color = Color::srgba(1.0, 0.4 + warm * 0.5, 0.1, 0.95);
            commands.spawn((
                Particle {
                    velocity: Vec2::new(
                        (rng.draw().unit() - 0.5) * 14.0,
                        24.0 + rng.draw().unit() * 26.0,
                    ),
                    gravity: -60.0,
                    ttl: FLAME_TTL,
                    max_ttl: FLAME_TTL,
                },
                Sprite::from_color(color, Vec2::ONE),
                Transform::from_translation((remote.pos + offset).extend(15.0)),
                RenderLayers::layer(WORLD_LAYER),
            ));
        }
    }
}

fn spawn_dig_spray(
    commands: &mut Commands,
    game: &crate::game::ClientGame,
    ingame: &crate::game::InGame,
    rng: &mut Rng,
) {
    let held = game.input.held;
    if !held.primary || !ingame.you.present {
        return;
    }
    let aim = Vec2::new(held.aim.x as f32, held.aim.y as f32);
    let reach = match ingame.you.mode {
        GameMode::Survival => SURVIVAL_REACH,
        GameMode::Creative => REACH,
    };
    if ingame.you.pos.distance_squared(aim) > reach * reach {
        return;
    }

    let registry = &game.registries.materials;
    let radius = ingame.inventory.brush as i32;
    let mut spawned = 0;
    for _ in 0..12 {
        if spawned >= SPRAY_PER_FRAME {
            break;
        }
        let span = (2 * radius + 1) as f32;
        let ox = (rng.draw().unit() * span) as i32 - radius;
        let oy = (rng.draw().unit() * span) as i32 - radius;
        if ox * ox + oy * oy > radius * radius {
            continue;
        }
        let pos = held.aim.translated(ox, oy);
        let Some(cell) = ingame.world.get_cell(pos) else {
            continue;
        };
        let material = registry.get(cell.material);
        if !matches!(material.phase, Phase::Solid | Phase::Powder) {
            continue;
        }
        let shade = (cell.shade_flags >> 4) as usize;
        let rgba = material.colors[shade % material.colors.len()];
        let color = Color::srgba_u8(rgba[0], rgba[1], rgba[2], 255);
        let angle = std::f32::consts::FRAC_PI_4 + rng.draw().unit() * std::f32::consts::FRAC_PI_2;
        let speed = 25.0 + rng.draw().unit() * 55.0;
        let velocity = Vec2::from_angle(angle) * speed;
        commands.spawn((
            Particle {
                velocity,
                gravity: SPRAY_GRAVITY,
                ttl: SPRAY_TTL,
                max_ttl: SPRAY_TTL,
            },
            Sprite::from_color(color, Vec2::ONE),
            Transform::from_xyz(pos.x as f32 + 0.5, pos.y as f32 + 0.5, 15.0),
            RenderLayers::layer(WORLD_LAYER),
        ));
        spawned += 1;
    }
}

pub fn update_particles(
    mut commands: Commands,
    game: Res<Game>,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Particle, &mut Transform, &mut Sprite)>,
) {
    let clear = game.0.ingame().is_none();
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform, mut sprite) in &mut query {
        particle.ttl -= dt;
        if clear || particle.ttl <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        particle.velocity.y -= particle.gravity * dt;
        transform.translation.x += particle.velocity.x * dt;
        transform.translation.y += particle.velocity.y * dt;
        let alpha = (particle.ttl / particle.max_ttl).clamp(0.0, 1.0);
        let color = sprite.color.with_alpha(alpha);
        sprite.color = color;
    }
}
