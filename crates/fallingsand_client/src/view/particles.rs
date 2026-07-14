use super::Game;
use super::PLAYER_WIDTH;
use super::camera::WORLD_LAYER;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use fallingsand_core::content;
use fallingsand_protocol::InteractionStatus;
use fallingsand_rng::Rng;

const SPRAY_TTL: f32 = 0.5;
const SPRAY_GRAVITY: f32 = 260.0;
const SPRAY_CHANCE: f32 = 0.2;
const FLAME_TTL: f32 = 0.45;
const FLAME_INTERVAL: f32 = 0.05;

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
    if ingame.game_menu_open() {
        return;
    }

    spawn_dig_spray(&mut commands, ingame, &mut rng);

    *flame_accum += time.delta_secs();
    if *flame_accum < FLAME_INTERVAL {
        return;
    }
    *flame_accum -= FLAME_INTERVAL;
    for remote in ingame.players.avatars.values() {
        if !remote.burning {
            continue;
        }
        let size = Vec2::new(PLAYER_WIDTH, remote.height.max(1) as f32);
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
                Transform::from_translation((remote.center() + offset).extend(15.0)),
                RenderLayers::layer(WORLD_LAYER),
            ));
        }
    }
}

fn spawn_dig_spray(commands: &mut Commands, ingame: &crate::game::InGame, rng: &mut Rng) {
    let Some(interaction) = ingame.you.life.avatar().map(|avatar| avatar.interaction) else {
        return;
    };
    let Some(material) = interaction.dig_material else {
        return;
    };
    if rng.draw().unit() > SPRAY_CHANCE {
        return;
    }
    let target = interaction.target;
    let center = Vec2::new(target.x as f32 + 0.5, target.y as f32 + 0.5);
    let colors = content::material(material).colors;
    let rgba = colors[rng.draw().range(0, colors.len() as i32 - 1) as usize];
    let color = Color::srgba_u8(rgba[0], rgba[1], rgba[2], 255);
    let angle = std::f32::consts::FRAC_PI_4 + rng.draw().unit() * std::f32::consts::FRAC_PI_2;
    let speed = 25.0 + rng.draw().unit() * 55.0;
    let velocity = Vec2::from_angle(angle) * speed;
    let jitter = Vec2::new(rng.draw().unit() - 0.5, rng.draw().unit() - 0.5);
    let origin = center + jitter;
    commands.spawn((
        Particle {
            velocity,
            gravity: SPRAY_GRAVITY,
            ttl: SPRAY_TTL,
            max_ttl: SPRAY_TTL,
        },
        Sprite::from_color(color, Vec2::ONE),
        Transform::from_xyz(origin.x, origin.y, 15.0),
        RenderLayers::layer(WORLD_LAYER),
    ));
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
