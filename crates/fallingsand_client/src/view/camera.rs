use super::Game;
use crate::game::RenderMode;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Hdr, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::Image;
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::ui::IsDefaultUiCamera;

pub const VIRTUAL_WIDTH: f32 = 424.0;
pub const WORLD_LAYER: usize = 1;
pub const SKY_LAYER: usize = 2;
pub const FAR_LAYER: usize = 3;
pub const NEAR_LAYER: usize = 4;
pub const WALL_LAYER: usize = 5;

#[derive(Component)]
pub struct WorldCamera;

#[derive(Component)]
pub struct SkyLayerCamera;

#[derive(Component)]
pub struct FarCamera;

#[derive(Component)]
pub struct NearCamera;

#[derive(Component)]
pub struct WallCamera;

#[derive(Component)]
pub struct CompositeCamera;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    World,
    Sky,
    Far,
    Near,
    Wall,
}

#[derive(Resource)]
pub struct LayerTargets {
    pub world: Handle<Image>,
    pub sky: Handle<Image>,
    pub far: Handle<Image>,
    pub near: Handle<Image>,
    pub wall: Handle<Image>,
}

impl LayerTargets {
    fn handle(&self, layer: Layer) -> &Handle<Image> {
        match layer {
            Layer::World => &self.world,
            Layer::Sky => &self.sky,
            Layer::Far => &self.far,
            Layer::Near => &self.near,
            Layer::Wall => &self.wall,
        }
    }

    fn set(&mut self, layer: Layer, handle: Handle<Image>) {
        match layer {
            Layer::World => self.world = handle,
            Layer::Sky => self.sky = handle,
            Layer::Far => self.far = handle,
            Layer::Near => self.near = handle,
            Layer::Wall => self.wall = handle,
        }
    }
}

#[derive(Component)]
pub struct LayerQuad {
    pub ratio: Vec2,
    pub z: f32,
}

#[derive(Resource)]
pub struct CameraState {
    pub pos: Vec2,
    pub k: u32,
    pub native: UVec2,
    pub window_px: UVec2,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            pos: Vec2::new(0.0, 24.0),
            k: 1,
            native: UVec2::ONE,
            window_px: UVec2::ONE,
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
    let k = (base as i32 + zoom_index).clamp((base / 2).max(1) as i32, (base * 2) as i32) as u32;
    let native = UVec2::new(
        (window_px.x.div_ceil(k) + 2).next_multiple_of(2),
        (window_px.y.div_ceil(k) + 2).next_multiple_of(2),
    );
    (k, native)
}

fn fixed_projection(size: UVec2) -> Projection {
    Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::Fixed {
            width: size.x as f32,
            height: size.y as f32,
        },
        ..OrthographicProjection::default_2d()
    })
}

fn native_target(images: &mut Assets<Image>, size: UVec2) -> Handle<Image> {
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0; 8],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    images.add(image)
}

fn native_camera(order: isize, layer: usize, native: UVec2, target: Handle<Image>) -> impl Bundle {
    (
        Camera2d,
        Hdr,
        Msaa::Off,
        Camera {
            order,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::from(target),
        RenderLayers::layer(layer),
        fixed_projection(native),
        Transform::IDENTITY,
    )
}

pub fn setup_camera(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    window: Single<&Window>,
) {
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

    let targets = LayerTargets {
        world: native_target(&mut images, native),
        sky: native_target(&mut images, native),
        far: native_target(&mut images, native),
        near: native_target(&mut images, native),
        wall: native_target(&mut images, native),
    };

    commands.spawn((
        native_camera(0, WORLD_LAYER, native, targets.world.clone()),
        Layer::World,
        WorldCamera,
    ));
    commands.spawn((
        native_camera(-1, SKY_LAYER, native, targets.sky.clone()),
        Layer::Sky,
        SkyLayerCamera,
    ));
    commands.spawn((
        native_camera(-4, FAR_LAYER, native, targets.far.clone()),
        Layer::Far,
        FarCamera,
    ));
    commands.spawn((
        native_camera(-3, NEAR_LAYER, native, targets.near.clone()),
        Layer::Near,
        NearCamera,
    ));
    commands.spawn((
        native_camera(-2, WALL_LAYER, native, targets.wall.clone()),
        Layer::Wall,
        WallCamera,
    ));
    commands.insert_resource(targets);

    commands.spawn((
        Camera2d,
        Hdr,
        Msaa::Off,
        Tonemapping::AcesFitted,
        Bloom {
            intensity: 0.55,
            prefilter: BloomPrefilter {
                threshold: 1.0,
                threshold_softness: 0.4,
            },
            ..Bloom::NATURAL
        },
        Camera {
            order: 1,
            ..default()
        },
        IsDefaultUiCamera,
        fixed_projection(window_px),
        Transform::IDENTITY,
        CompositeCamera,
    ));
}

#[allow(clippy::type_complexity)]
pub fn sync_camera(
    game: Res<Game>,
    time: Res<Time>,
    window: Single<&Window>,
    mut state: ResMut<CameraState>,
    mut composite: Single<&mut Projection, With<CompositeCamera>>,
    mut world_camera: Single<&mut Transform, (With<WorldCamera>, Without<LayerQuad>)>,
    mut quads: Query<(&LayerQuad, &mut Transform), Without<WorldCamera>>,
) {
    let window_px = UVec2::new(
        window.physical_width().max(1),
        window.physical_height().max(1),
    );
    let (k, native) = pixel_scale(window_px, game.0.view_prefs.zoom_index);
    if state.k != k || state.native != native || state.window_px != window_px {
        if state.window_px != window_px {
            **composite = fixed_projection(window_px);
        }
        state.k = k;
        state.native = native;
        state.window_px = window_px;
    }

    match game.0.player_pos() {
        Some(target) => {
            let blend = 1.0 - (-8.0 * time.delta_secs()).exp();
            state.pos = state.pos.lerp(target, blend);
        }
        None if game.0.ingame().is_none() => {
            state.pos = CameraState::default().pos;
        }
        None => {}
    }

    let (snapped, _) = state.layer(Vec2::ZERO);
    world_camera.translation.x = snapped.x as f32;
    world_camera.translation.y = snapped.y as f32;

    let size = (state.native * state.k).as_vec2();
    for (quad, mut transform) in &mut quads {
        let (_, remainder_px) = state.layer(quad.ratio);
        let offset = match game.0.view_prefs.render_mode {
            RenderMode::PixelPerfect => -remainder_px.round(),
            RenderMode::Smooth => -remainder_px,
            RenderMode::Retro => Vec2::ZERO,
        };
        transform.translation = offset.extend(quad.z);
        transform.scale = Vec3::new(size.x, size.y, 1.0);
    }
}

pub fn resize_targets(
    state: Res<CameraState>,
    mut last: Local<UVec2>,
    mut images: ResMut<Assets<Image>>,
    mut targets: ResMut<LayerTargets>,
    mut cams: Query<(&Layer, &mut Projection, &mut RenderTarget)>,
) {
    if *last == state.native {
        return;
    }
    *last = state.native;

    for (layer, mut projection, mut target) in &mut cams {
        let handle = native_target(&mut images, state.native);
        images.remove(targets.handle(*layer));
        targets.set(*layer, handle.clone());
        *projection = fixed_projection(state.native);
        *target = RenderTarget::from(handle);
    }
}

pub fn cursor_to_world(window: &Window, state: &CameraState) -> Option<Vec2> {
    let cursor = window.cursor_position()? * window.scale_factor();
    let centered = cursor - state.window_px.as_vec2() / 2.0;
    Some(state.pos + Vec2::new(centered.x, -centered.y) / state.k as f32)
}
