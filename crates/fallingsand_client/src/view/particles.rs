use super::Game;
use super::camera::WORLD_LAYER;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use fallingsand_protocol::{InteractionStatus, ParticleSpawn};

const SPRAY_TTL: f32 = 0.5;
const SPRAY_GRAVITY: f32 = 260.0;

#[derive(Component)]
pub struct Particle {
    velocity: Vec2,
    gravity: f32,
    ttl: f32,
    max_ttl: f32,
}

#[derive(Component)]
pub struct TargetHighlight;

pub fn sync_target(
    mut commands: Commands,
    game: Res<Game>,
    mut query: Query<(&mut Transform, &mut Sprite, &mut Visibility), With<TargetHighlight>>,
) {
    let shown = game
        .0
        .playing()
        .and_then(|ingame| ingame.you.life.avatar().map(|avatar| avatar.interaction))
        .and_then(|state| status_color(state.status, state.progress).map(|color| (state, color)));

    if query.is_empty() {
        commands.spawn((
            TargetHighlight,
            Sprite::from_color(shown.map_or(Color::NONE, |(_, color)| color), Vec2::ONE),
            Transform::from_xyz(0.0, 0.0, 14.0),
            RenderLayers::layer(WORLD_LAYER),
            if shown.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ));
        return;
    }
    for (mut transform, mut sprite, mut visibility) in &mut query {
        let Some((state, color)) = shown else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Inherited;
        transform.translation.x = state.target.x as f32 + 0.5;
        transform.translation.y = state.target.y as f32 + 0.5;
        sprite.color = color;
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

pub fn drain_particles(mut commands: Commands, mut game: ResMut<Game>) {
    let Some(ingame) = game.0.ingame_mut() else {
        return;
    };
    for spawn in ingame.particles.drain(..) {
        commands.spawn(particle_bundle(spawn));
    }
}

fn particle_bundle(spawn: ParticleSpawn) -> (Particle, Sprite, Transform, RenderLayers) {
    let color = Color::srgb_u8(spawn.color[0], spawn.color[1], spawn.color[2]);
    (
        Particle {
            velocity: Vec2::new(spawn.vx, spawn.vy),
            gravity: SPRAY_GRAVITY,
            ttl: SPRAY_TTL,
            max_ttl: SPRAY_TTL,
        },
        Sprite::from_color(color, Vec2::ONE),
        Transform::from_xyz(spawn.x, spawn.y, 15.0),
        RenderLayers::layer(WORLD_LAYER),
    )
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
