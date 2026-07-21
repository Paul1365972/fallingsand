use super::Game;
use crate::game::RenderMode;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;
use bevy::ui::IsDefaultUiCamera;
use fallingsand_core::Calendar;

pub const VIRTUAL_WIDTH: f32 = 424.0;
pub const GLOW_RADIUS: f32 = 50.0;
pub const AIR_RADIUS: f32 = 35.0;
pub const LIGHT_MARGIN: u32 = 50;
pub const LIGHT_FIELD_DOWNSCALE: u32 = 4;
pub const FIELD_TAP_RADIUS: usize = 13;
pub const FIELD_TAP_COUNT: usize = 2 * FIELD_TAP_RADIUS + 1;
pub const FIELD_TAP_VEC4S: usize = FIELD_TAP_COUNT.div_ceil(4);
pub const FAR_RATIO: Vec2 = Vec2::new(0.88, 0.92);
pub const NEAR_RATIO: Vec2 = Vec2::new(0.72, 0.80);
pub const WALL_RATIO: Vec2 = Vec2::splat(0.15);
pub const STAR_WORLD_TILE: f32 = 512.0;

pub fn star_scroll(calendar: Calendar) -> Vec2 {
    Vec2::new(
        (-calendar.sidereal() * STAR_WORLD_TILE).rem_euclid(STAR_WORLD_TILE),
        0.0,
    )
}

#[derive(ShaderType, Debug, Clone)]
pub struct LightBlurParams {
    pub glow_weights: [Vec4; FIELD_TAP_VEC4S],
    pub air_weights: [Vec4; FIELD_TAP_VEC4S],
}

fn gaussian_kernel_sum(radius: f32) -> f32 {
    let sigma = radius / 3.0;
    let r = radius.ceil() as i32;
    (-r..=r)
        .map(|d| (-((d * d) as f32) / (2.0 * sigma * sigma)).exp())
        .sum()
}

fn field_weights(radius: f32, kernel_sum: f32) -> [Vec4; FIELD_TAP_VEC4S] {
    let sigma = radius / (3.0 * LIGHT_FIELD_DOWNSCALE as f32);
    let taps: [f32; FIELD_TAP_COUNT] = std::array::from_fn(|i| {
        let d = i as f32 - FIELD_TAP_RADIUS as f32;
        (-(d * d) / (2.0 * sigma * sigma)).exp()
    });
    let scale = kernel_sum / taps.iter().sum::<f32>();
    std::array::from_fn(|v| {
        let tap = |i: usize| taps.get(v * 4 + i).copied().unwrap_or(0.0);
        Vec4::new(tap(0), tap(1), tap(2), tap(3)) * scale
    })
}

pub fn light_blur_params() -> LightBlurParams {
    LightBlurParams {
        glow_weights: field_weights(GLOW_RADIUS, gaussian_kernel_sum(GLOW_RADIUS)),
        air_weights: field_weights(AIR_RADIUS, 1.0),
    }
}

pub fn extended_size(native: UVec2) -> UVec2 {
    UVec2::new(
        (native.x + 2 * LIGHT_MARGIN).next_multiple_of(LIGHT_FIELD_DOWNSCALE),
        (native.y + 2 * LIGHT_MARGIN).next_multiple_of(LIGHT_FIELD_DOWNSCALE),
    )
}

pub fn light_field_margin(native: UVec2) -> Vec2 {
    ((extended_size(native) - native) / 2).as_vec2()
}

#[derive(Resource)]
pub struct CameraState {
    pub pos: Vec2,
    pub k: u32,
    pub native: UVec2,
    pub window_px: UVec2,
    pub star_scroll: Vec2,
    pub render_mode: RenderMode,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            pos: Vec2::new(0.0, 24.0),
            k: 1,
            native: UVec2::ONE,
            window_px: UVec2::ONE,
            star_scroll: Vec2::ZERO,
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

    let calendar = game
        .0
        .ingame()
        .map(|ingame| ingame.clock.calendar)
        .unwrap_or_default();
    state.star_scroll = star_scroll(calendar);
}

pub fn cursor_to_world(window: &Window, state: &CameraState) -> Option<Vec2> {
    let cursor = window.cursor_position()? * window.scale_factor();
    let centered = cursor - state.window_px.as_vec2() / 2.0;
    Some(state.pos + Vec2::new(centered.x, -centered.y) / state.k as f32)
}
