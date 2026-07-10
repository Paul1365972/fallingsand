use crate::camera::WORLD_LAYER;
use crate::input::InputHeld;
use crate::inventory::BrushRadius;
use crate::player::{LocalPlayerState, PlayerVisual, PlayerVisuals};
use crate::worldview::WorldView;
use crate::{AppState, ClientRegistry, GameState};
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use fallingsand_core::{Phase, REACH, SURVIVAL_REACH};
use fallingsand_protocol::GameMode;
use fallingsand_rng::Rng;

pub struct ParticlesPlugin;

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

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (spawn_dig_spray, spawn_flames).run_if(in_state(crate::PauseState::Running)),
                update_particles.run_if(in_state(GameState::Playing)),
            ),
        )
        .add_systems(OnExit(AppState::InGame), cleanup_particles);
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_dig_spray(
    mut commands: Commands,
    held: Res<InputHeld>,
    state: Res<LocalPlayerState>,
    view: Res<WorldView>,
    registry: Res<ClientRegistry>,
    session: Option<Res<crate::net::Session>>,
    visuals: Res<PlayerVisuals>,
    transforms: Query<&Transform, With<PlayerVisual>>,
    brush: Res<BrushRadius>,
    mut rng: Local<Rng>,
) {
    if !held.0.primary {
        return;
    }
    let Some(id) = session.and_then(|session| session.player) else {
        return;
    };
    let Some(&entity) = visuals.0.get(&id) else {
        return;
    };
    let Ok(player) = transforms.get(entity) else {
        return;
    };
    let aim = Vec2::new(held.0.aim.x as f32, held.0.aim.y as f32);
    let reach = match state.mode {
        GameMode::Survival => SURVIVAL_REACH,
        GameMode::Creative => REACH,
    };
    if player.translation.truncate().distance_squared(aim) > reach * reach {
        return;
    }

    let radius = brush.0 as i32;
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
        let pos = held.0.aim.translated(ox, oy);
        let Some(cell) = view.get_cell(pos) else {
            continue;
        };
        let material = registry.0.get(cell.material);
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

fn spawn_flames(
    mut commands: Commands,
    time: Res<Time>,
    query: Query<(&Transform, &PlayerVisual)>,
    mut accumulator: Local<f32>,
    mut rng: Local<Rng>,
) {
    *accumulator += time.delta_secs();
    if *accumulator < FLAME_INTERVAL {
        return;
    }
    *accumulator = 0.0;
    for (transform, visual) in &query {
        if !visual.burning {
            continue;
        }
        let size = if visual.ducking {
            crate::player::PLAYER_DUCK_SIZE
        } else {
            crate::player::PLAYER_SIZE
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
                Transform::from_translation(
                    (transform.translation.truncate() + offset).extend(15.0),
                ),
                RenderLayers::layer(WORLD_LAYER),
            ));
        }
    }
}

fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Particle, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform, mut sprite) in &mut query {
        particle.ttl -= dt;
        if particle.ttl <= 0.0 {
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

fn cleanup_particles(mut commands: Commands, query: Query<Entity, With<Particle>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
