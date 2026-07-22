use super::super::Game;
use crate::view::camera::CameraState;
use bevy::prelude::*;
use fallingsand_core::{CARDINAL_NEIGHBORS, CHUNK_SIZE, REGION_SIZE_CELLS};
use fallingsand_protocol::InteractionStatus;
use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Clone, Copy)]
pub struct DebugLine {
    pub a: Vec2,
    pub b: Vec2,
    pub color: Vec4,
}

#[derive(Resource, Default)]
pub struct DebugPrimitives {
    pub lines: Vec<DebugLine>,
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

pub(super) fn update_particles(
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

pub(super) fn update_debug_primitives(
    game: Res<Game>,
    state: Res<CameraState>,
    mut primitives: ResMut<DebugPrimitives>,
) {
    primitives.lines.clear();
    if !game.0.view_prefs.debug_borders {
        return;
    }
    let Some(ingame) = game.0.ingame() else {
        return;
    };
    let half = state.view_cells() / 2.0;
    let min = state.pos - half;
    let max = state.pos + half;
    let k = state.k as f32;
    let to_px = |world: Vec2| (world - state.pos) * k;

    let chunk = CHUNK_SIZE as f32;
    let region = REGION_SIZE_CELLS as f32;
    let chunk_color = Color::srgba(1.0, 1.0, 1.0, 0.12);
    let region_color = Color::srgba(1.0, 0.55, 0.2, 0.6);
    let body_color: Vec4 = Color::srgba(0.2, 1.0, 0.85, 0.95)
        .to_linear()
        .to_f32_array()
        .into();

    let mut x = (min.x / chunk).floor() * chunk;
    while x <= max.x {
        let color = if x.rem_euclid(region) == 0.0 {
            region_color
        } else {
            chunk_color
        };
        primitives.lines.push(DebugLine {
            a: to_px(Vec2::new(x, min.y)),
            b: to_px(Vec2::new(x, max.y)),
            color: color.to_linear().to_f32_array().into(),
        });
        x += chunk;
    }
    let mut y = (min.y / chunk).floor() * chunk;
    while y <= max.y {
        let color = if y.rem_euclid(region) == 0.0 {
            region_color
        } else {
            chunk_color
        };
        primitives.lines.push(DebugLine {
            a: to_px(Vec2::new(min.x, y)),
            b: to_px(Vec2::new(max.x, y)),
            color: color.to_linear().to_f32_array().into(),
        });
        y += chunk;
    }

    let mut bodies: BTreeMap<u32, BTreeSet<_>> = BTreeMap::new();
    for &(body, cell) in &ingame.debug.body_cells {
        bodies.entry(body).or_default().insert(cell);
    }
    for body_cells in bodies.values() {
        for &cell in body_cells {
            let x = cell.x as f32;
            let y = cell.y as f32;
            for (dx, dy) in CARDINAL_NEIGHBORS {
                if body_cells.contains(&cell.translated(dx, dy)) {
                    continue;
                }
                let (a, b) = match (dx, dy) {
                    (0, -1) => (Vec2::new(x, y), Vec2::new(x + 1.0, y)),
                    (-1, 0) => (Vec2::new(x, y), Vec2::new(x, y + 1.0)),
                    (1, 0) => (Vec2::new(x + 1.0, y), Vec2::new(x + 1.0, y + 1.0)),
                    (0, 1) => (Vec2::new(x, y + 1.0), Vec2::new(x + 1.0, y + 1.0)),
                    _ => unreachable!(),
                };
                primitives.lines.push(DebugLine {
                    a: to_px(a),
                    b: to_px(b),
                    color: body_color,
                });
            }
        }
    }

    for flash in &ingame.debug.rects {
        let origin = Vec2::new(flash.pos.x as f32 * chunk, flash.pos.y as f32 * chunk);
        let corner = origin + Vec2::new(flash.rect.min_x as f32, flash.rect.min_y as f32);
        let size = Vec2::new(flash.rect.width() as f32, flash.rect.height() as f32);
        let color = if flash.is_sim {
            Color::srgba(0.2, 0.9, 1.0, 0.8)
        } else {
            Color::srgba(1.0, 0.9, 0.2, 0.8)
        };
        let min = to_px(corner);
        let max = to_px(corner + size);
        let color: Vec4 = color.to_linear().to_f32_array().into();
        primitives.lines.extend([
            DebugLine {
                a: min,
                b: Vec2::new(max.x, min.y),
                color,
            },
            DebugLine {
                a: Vec2::new(max.x, min.y),
                b: max,
                color,
            },
            DebugLine {
                a: max,
                b: Vec2::new(min.x, max.y),
                color,
            },
            DebugLine {
                a: Vec2::new(min.x, max.y),
                b: min,
                color,
            },
        ]);
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
