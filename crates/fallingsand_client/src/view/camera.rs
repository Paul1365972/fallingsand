use super::Game;
use crate::game::RenderMode;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

pub const VIRTUAL_WIDTH: f32 = 424.0;

#[derive(Resource)]
pub struct CameraState {
    pub pos: Vec2,
    pub k: u32,
    pub native: UVec2,
    pub window_px: UVec2,
    pub render_mode: RenderMode,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            pos: Vec2::new(0.0, 24.0),
            k: 1,
            native: UVec2::ONE,
            window_px: UVec2::ONE,
            render_mode: RenderMode::PixelPerfect,
        }
    }
}

impl CameraState {
    pub fn layer(&self, ratio: Vec2) -> (IVec2, Vec2) {
        let cam = self.pos * (Vec2::ONE - ratio);
        let snapped = cam.floor();
        let remainder = cam - snapped;
        (snapped.as_ivec2(), remainder * self.k as f32)
    }

    pub fn view_cells(&self) -> Vec2 {
        self.window_px.as_vec2() / self.k as f32
    }
}

pub fn base_scale(window_px: UVec2) -> u32 {
    ((window_px.x as f32 / VIRTUAL_WIDTH).round() as u32).max(1)
}

fn pixel_scale(window_px: UVec2, zoom_index: i32) -> (u32, UVec2) {
    let base = base_scale(window_px);
    let k = (base as i32 + crate::game::input::clamp_zoom(base, zoom_index)) as u32;
    let native = UVec2::new(
        (window_px.x.div_ceil(k) + 2).next_multiple_of(2),
        (window_px.y.div_ceil(k) + 2).next_multiple_of(2),
    );
    (k, native)
}

pub fn setup_camera(mut commands: Commands, window: Single<&Window>) {
    let window_px = UVec2::new(
        window.physical_width().max(1),
        window.physical_height().max(1),
    );
    let (k, native) = pixel_scale(window_px, 0);
    commands.insert_resource(CameraState {
        k,
        native,
        window_px,
        ..default()
    });
    commands.spawn((
        Camera2d,
        Hdr,
        Msaa::Off,
        Tonemapping::AcesFitted,
        Bloom {
            intensity: 0.72,
            prefilter: BloomPrefilter {
                threshold: 1.0,
                threshold_softness: 0.4,
            },
            ..Bloom::NATURAL
        },
        IsDefaultUiCamera,
    ));
}

pub fn sync_camera(
    game: Res<Game>,
    time: Res<Time>,
    window: Single<&Window>,
    mut state: ResMut<CameraState>,
) {
    let window_px = UVec2::new(
        window.physical_width().max(1),
        window.physical_height().max(1),
    );
    let (k, native) = pixel_scale(window_px, game.0.view_prefs.zoom_index);
    state.k = k;
    state.native = native;
    state.window_px = window_px;
    state.render_mode = game.0.settings.render_mode;

    match game.0.player_pos() {
        Some(target) => {
            let blend = 1.0 - (-8.0 * time.delta_secs()).exp();
            state.pos = state.pos.lerp(target, blend);
        }
        None if game.0.ingame().is_none() => state.pos = CameraState::default().pos,
        None => {}
    }
}

pub fn cursor_to_world(window: &Window, state: &CameraState) -> Option<Vec2> {
    let cursor = window.cursor_position()? * window.scale_factor();
    let centered = cursor - state.window_px.as_vec2() / 2.0;
    Some(state.pos + Vec2::new(centered.x, -centered.y) / state.k as f32)
}
