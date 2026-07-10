use crate::camera::CameraState;
use crate::net::{ServerMsg, Session, TickMessage};
use bevy::prelude::*;
use fallingsand_core::{CHUNK_SIZE, ChunkPos, DirtyRect, REGION_SIZE_CELLS};

#[derive(Resource, Default)]
pub(super) struct BordersVisible(pub(super) bool);

struct RectFlash {
    pos: ChunkPos,
    rect: DirtyRect,
    is_sim: bool,
}

#[derive(Resource, Default)]
pub(super) struct RectFlashes(Vec<RectFlash>);

pub(super) fn sync_debug_stream(
    borders: Res<BordersVisible>,
    session: Option<ResMut<Session>>,
    mut messages: MessageReader<ServerMsg>,
    mut subscribed: Local<bool>,
) {
    let Some(mut session) = session else {
        *subscribed = false;
        return;
    };
    let rejoined = messages.read().any(|ServerMsg(message)| {
        matches!(
            message,
            fallingsand_protocol::ServerMessage::HelloAck { .. }
        )
    });
    if rejoined {
        *subscribed = false;
    }
    if session.player.is_some() && *subscribed != borders.0 {
        session.send(&fallingsand_protocol::ClientMessage::SetDebug { enabled: borders.0 });
        *subscribed = borders.0;
    }
}

pub(super) fn track_rects(
    mut flashes: ResMut<RectFlashes>,
    mut frames: MessageReader<TickMessage>,
    borders: Res<BordersVisible>,
) {
    if !borders.0 {
        frames.clear();
        if !flashes.0.is_empty() {
            flashes.0.clear();
        }
        return;
    }
    let Some(TickMessage(tick)) = frames.read().last() else {
        return;
    };
    flashes.0.clear();
    for entry in &tick.debug {
        for (rect, is_sim) in [(entry.change, false), (entry.sim, true)] {
            if rect.is_empty() || (is_sim && entry.sim == entry.change) {
                continue;
            }
            flashes.0.push(RectFlash {
                pos: entry.pos,
                rect,
                is_sim,
            });
        }
    }
}

pub(super) fn draw_borders(
    borders: Res<BordersVisible>,
    flashes: Res<RectFlashes>,
    state: Res<CameraState>,
    mut gizmos: Gizmos,
) {
    if !borders.0 {
        return;
    }
    let half = state.view_cells() / 2.0;
    let min = state.pos - half;
    let max = state.pos + half;
    let k = state.k as f32;
    let to_px = |world: Vec2| (world - state.pos) * k;

    let chunk = CHUNK_SIZE as f32;
    let region = REGION_SIZE_CELLS as f32;
    let chunk_color = Color::srgba(1.0, 1.0, 1.0, 0.12);
    let region_color = Color::srgba(1.0, 0.55, 0.2, 0.6);

    let mut x = (min.x / chunk).floor() * chunk;
    while x <= max.x {
        let color = if x.rem_euclid(region) == 0.0 {
            region_color
        } else {
            chunk_color
        };
        gizmos.line_2d(
            to_px(Vec2::new(x, min.y)),
            to_px(Vec2::new(x, max.y)),
            color,
        );
        x += chunk;
    }
    let mut y = (min.y / chunk).floor() * chunk;
    while y <= max.y {
        let color = if y.rem_euclid(region) == 0.0 {
            region_color
        } else {
            chunk_color
        };
        gizmos.line_2d(
            to_px(Vec2::new(min.x, y)),
            to_px(Vec2::new(max.x, y)),
            color,
        );
        y += chunk;
    }

    for flash in &flashes.0 {
        let origin = Vec2::new(flash.pos.x as f32 * chunk, flash.pos.y as f32 * chunk);
        let corner = origin + Vec2::new(flash.rect.min_x as f32, flash.rect.min_y as f32);
        let size = Vec2::new(flash.rect.width() as f32, flash.rect.height() as f32);
        let color = if flash.is_sim {
            Color::srgba(0.2, 0.9, 1.0, 0.8)
        } else {
            Color::srgba(1.0, 0.9, 0.2, 0.8)
        };
        gizmos.rect_2d(
            Isometry2d::from_translation(to_px(corner + size / 2.0)),
            size * k,
            color,
        );
    }
}
