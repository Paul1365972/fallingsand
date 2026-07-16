use super::Game;
use super::sky::{LightingMaterial, LightingParams};
use crate::game::RenderMode;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Hdr, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::{Image, ImageSampler};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendState, Extent3d, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey};
use bevy::ui::IsDefaultUiCamera;
use fallingsand_core::Calendar;

pub const VIRTUAL_WIDTH: f32 = 424.0;

pub const WORLD_LAYER: usize = 1;
pub const STAR_LAYER: usize = 2;
pub const SKY_LAYER: usize = 3;
pub const FAR_LAYER: usize = 4;
pub const NEAR_LAYER: usize = 5;
pub const WALL_LAYER: usize = 6;
pub const EMISSIVE_LAYER: usize = 7;

pub const L_WORLD: usize = 0;
pub const L_STAR: usize = 1;
pub const L_SKY: usize = 2;
pub const L_FAR: usize = 3;
pub const L_NEAR: usize = 4;
pub const L_WALL: usize = 5;
pub const L_EMISSIVE_SRC: usize = 6;
const L_LIGHT_HALF: usize = 7;
const L_LIGHT_QUARTER: usize = 8;
const L_LIGHT_TMP: usize = 9;
const L_LIGHT: usize = 10;
pub const TARGET_COUNT: usize = 11;

const GLOW_RADIUS: f32 = 50.0;
const AIR_RADIUS: f32 = 35.0;
const LIGHT_MARGIN: u32 = 50;
const LIGHT_FIELD_DOWNSCALE: u32 = 4;
const FIELD_TAP_RADIUS: usize = 13;
const FIELD_TAP_COUNT: usize = 2 * FIELD_TAP_RADIUS + 1;
const FIELD_TAP_VEC4S: usize = FIELD_TAP_COUNT.div_ceil(4);

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

pub struct LayerDef {
    pub render_layer: usize,
    pub ratio: Vec2,
    pub z: f32,
    pub follow: bool,
    pub lit: bool,
    pub drift: bool,
}

pub const LAYERS: [LayerDef; 6] = [
    LayerDef {
        render_layer: WORLD_LAYER,
        ratio: Vec2::ZERO,
        z: 0.0,
        follow: true,
        lit: true,
        drift: false,
    },
    LayerDef {
        render_layer: STAR_LAYER,
        ratio: Vec2::ONE,
        z: -46.0,
        follow: false,
        lit: false,
        drift: true,
    },
    LayerDef {
        render_layer: SKY_LAYER,
        ratio: Vec2::ONE,
        z: -44.0,
        follow: false,
        lit: false,
        drift: false,
    },
    LayerDef {
        render_layer: FAR_LAYER,
        ratio: FAR_RATIO,
        z: -40.0,
        follow: false,
        lit: false,
        drift: false,
    },
    LayerDef {
        render_layer: NEAR_LAYER,
        ratio: NEAR_RATIO,
        z: -38.0,
        follow: false,
        lit: false,
        drift: false,
    },
    LayerDef {
        render_layer: WALL_LAYER,
        ratio: WALL_RATIO,
        z: -20.0,
        follow: false,
        lit: false,
        drift: false,
    },
];

#[derive(Component)]
pub struct WorldCamera;

#[derive(Component)]
pub struct EmissiveCamera;

#[derive(Component)]
pub struct CompositeCamera;

#[derive(Component)]
pub struct PassQuad(usize);

#[derive(Component)]
pub struct LayerQuad {
    pub ratio: Vec2,
    pub z: f32,
    pub drift: bool,
}

#[derive(Resource)]
pub struct LayerTargets {
    handles: [Handle<Image>; TARGET_COUNT],
    native: UVec2,
}

#[derive(Resource)]
pub struct LayerAssets {
    pub lighting: Handle<LightingMaterial>,
    upscale: [Option<Handle<UpscaleMaterial>>; 6],
    down_half: Handle<DownsampleMaterial>,
    down_quarter: Handle<DownsampleMaterial>,
    light_blur_h: Handle<LightBlurMaterial>,
    light_blur_v: Handle<LightBlurMaterial>,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct DownsampleMaterial {
    #[texture(0)]
    pub src: Handle<Image>,
}

impl Material2d for DownsampleMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/downsample.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}

#[derive(ShaderType, Debug, Clone)]
pub struct LightBlurParams {
    pub glow_weights: [Vec4; FIELD_TAP_VEC4S],
    pub air_weights: [Vec4; FIELD_TAP_VEC4S],
    pub dir: Vec2,
    pub _pad: Vec2,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct LightBlurMaterial {
    #[uniform(0)]
    pub params: LightBlurParams,
    #[texture(1)]
    pub src: Handle<Image>,
}

impl Material2d for LightBlurMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/light_blur.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
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

fn light_blur_params(dir: Vec2) -> LightBlurParams {
    LightBlurParams {
        glow_weights: field_weights(GLOW_RADIUS, gaussian_kernel_sum(GLOW_RADIUS)),
        air_weights: field_weights(AIR_RADIUS, 1.0),
        dir,
        _pad: Vec2::ZERO,
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct UpscaleMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
}

impl Material2d for UpscaleMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/upscale.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        premultiplied_composite(descriptor);
        Ok(())
    }
}

pub fn premultiplied_composite(descriptor: &mut RenderPipelineDescriptor) {
    if let Some(fragment) = &mut descriptor.fragment {
        for target in fragment.targets.iter_mut().flatten() {
            target.blend = Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        }
    }
}

#[derive(Resource)]
pub struct CameraState {
    pub pos: Vec2,
    pub k: u32,
    pub native: UVec2,
    pub window_px: UVec2,
    pub star_scroll: Vec2,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            pos: Vec2::new(0.0, 24.0),
            k: 1,
            native: UVec2::ONE,
            window_px: UVec2::ONE,
            star_scroll: Vec2::ZERO,
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

#[derive(Component)]
pub struct LayerCamera(pub usize);

pub fn layer_camera(cameras: &Query<(Entity, &LayerCamera)>, index: usize) -> Option<Entity> {
    cameras
        .iter()
        .find(|(_, layer)| layer.0 == index)
        .map(|(entity, _)| entity)
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

fn extended_size(native: UVec2) -> UVec2 {
    UVec2::new(
        (native.x + 2 * LIGHT_MARGIN).next_multiple_of(LIGHT_FIELD_DOWNSCALE),
        (native.y + 2 * LIGHT_MARGIN).next_multiple_of(LIGHT_FIELD_DOWNSCALE),
    )
}

pub fn light_field_margin(native: UVec2) -> Vec2 {
    ((extended_size(native) - native) / 2).as_vec2()
}

fn target_size(layer: usize, native: UVec2) -> UVec2 {
    match layer {
        L_EMISSIVE_SRC => extended_size(native),
        L_LIGHT_HALF => extended_size(native) / 2,
        L_LIGHT_QUARTER | L_LIGHT_TMP | L_LIGHT => extended_size(native) / LIGHT_FIELD_DOWNSCALE,
        _ => native,
    }
}

fn target_sampler(layer: usize) -> ImageSampler {
    if layer == L_LIGHT {
        ImageSampler::linear()
    } else {
        ImageSampler::Default
    }
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

fn native_target(images: &mut Assets<Image>, size: UVec2, sampler: ImageSampler) -> Handle<Image> {
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
    image.sampler = sampler;
    images.add(image)
}

fn native_camera(order: isize, layer: usize, size: UVec2, target: Handle<Image>) -> impl Bundle {
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
        fixed_projection(size),
        Transform::IDENTITY,
    )
}

fn spawn_pass<M: Material2d>(
    commands: &mut Commands,
    quad: &Handle<Mesh>,
    order: isize,
    target_index: usize,
    native: UVec2,
    target: Handle<Image>,
    material: Handle<M>,
) {
    let render_layer = target_index + 1;
    let size = target_size(target_index, native);
    commands
        .spawn((
            native_camera(order, render_layer, size, target),
            LayerCamera(target_index),
        ))
        .with_children(|parent| {
            parent.spawn((
                PassQuad(target_index),
                Mesh2d(quad.clone()),
                MeshMaterial2d(material),
                Transform::from_scale(Vec3::new(size.x as f32, size.y as f32, 1.0)),
                RenderLayers::layer(render_layer),
            ));
        });
}

#[allow(clippy::too_many_arguments)]
pub fn setup_camera(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut upscale_mats: ResMut<Assets<UpscaleMaterial>>,
    mut lighting_mats: ResMut<Assets<LightingMaterial>>,
    mut downsample_mats: ResMut<Assets<DownsampleMaterial>>,
    mut light_blur_mats: ResMut<Assets<LightBlurMaterial>>,
    shared: Res<super::chunks::RenderShared>,
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

    let targets: [Handle<Image>; TARGET_COUNT] = std::array::from_fn(|i| {
        native_target(&mut images, target_size(i, native), target_sampler(i))
    });

    for (i, def) in LAYERS.iter().enumerate() {
        let mut camera = commands.spawn((
            native_camera(-(i as isize), def.render_layer, native, targets[i].clone()),
            LayerCamera(i),
        ));
        if def.follow {
            camera.insert(WorldCamera);
        }
    }

    commands.spawn((
        native_camera(
            -10,
            EMISSIVE_LAYER,
            target_size(L_EMISSIVE_SRC, native),
            targets[L_EMISSIVE_SRC].clone(),
        ),
        LayerCamera(L_EMISSIVE_SRC),
        EmissiveCamera,
    ));

    let down_half = downsample_mats.add(DownsampleMaterial {
        src: targets[L_EMISSIVE_SRC].clone(),
    });
    let down_quarter = downsample_mats.add(DownsampleMaterial {
        src: targets[L_LIGHT_HALF].clone(),
    });
    let light_blur_h = light_blur_mats.add(LightBlurMaterial {
        params: light_blur_params(Vec2::X),
        src: targets[L_LIGHT_QUARTER].clone(),
    });
    let light_blur_v = light_blur_mats.add(LightBlurMaterial {
        params: light_blur_params(Vec2::Y),
        src: targets[L_LIGHT_TMP].clone(),
    });
    spawn_pass(
        &mut commands,
        &shared.quad,
        -9,
        L_LIGHT_HALF,
        native,
        targets[L_LIGHT_HALF].clone(),
        down_half.clone(),
    );
    spawn_pass(
        &mut commands,
        &shared.quad,
        -8,
        L_LIGHT_QUARTER,
        native,
        targets[L_LIGHT_QUARTER].clone(),
        down_quarter.clone(),
    );
    spawn_pass(
        &mut commands,
        &shared.quad,
        -7,
        L_LIGHT_TMP,
        native,
        targets[L_LIGHT_TMP].clone(),
        light_blur_h.clone(),
    );
    spawn_pass(
        &mut commands,
        &shared.quad,
        -6,
        L_LIGHT,
        native,
        targets[L_LIGHT].clone(),
        light_blur_v.clone(),
    );

    let composite = commands
        .spawn((
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
            Camera {
                order: 1,
                ..default()
            },
            IsDefaultUiCamera,
            fixed_projection(window_px),
            Transform::IDENTITY,
            CompositeCamera,
        ))
        .id();

    let quad = shared.quad.clone();
    let lighting = lighting_mats.add(LightingMaterial {
        params: LightingParams {
            margin: light_field_margin(native),
            ..default()
        },
        world: targets[L_WORLD].clone(),
        light: targets[L_LIGHT].clone(),
        emission: targets[L_EMISSIVE_SRC].clone(),
    });
    let mut upscale: [Option<Handle<UpscaleMaterial>>; 6] = Default::default();
    commands.entity(composite).with_children(|parent| {
        for (i, def) in LAYERS.iter().enumerate() {
            let mut quad = parent.spawn((
                LayerQuad {
                    ratio: def.ratio,
                    z: def.z,
                    drift: def.drift,
                },
                Mesh2d(quad.clone()),
                Transform::from_xyz(0.0, 0.0, def.z),
            ));
            if def.lit {
                quad.insert(MeshMaterial2d(lighting.clone()));
            } else {
                let material = upscale_mats.add(UpscaleMaterial {
                    texture: targets[i].clone(),
                });
                upscale[i] = Some(material.clone());
                quad.insert(MeshMaterial2d(material));
            }
        }
    });

    commands.insert_resource(LayerTargets {
        handles: targets,
        native,
    });
    commands.insert_resource(LayerAssets {
        lighting,
        upscale,
        down_half,
        down_quarter,
        light_blur_h,
        light_blur_v,
    });
}

pub fn sync_pass_activity(
    game: Res<Game>,
    mut cameras: Query<&mut Camera, With<LayerCamera>>,
    mut quads: Query<&mut Visibility, With<LayerQuad>>,
) {
    let active = game.0.ingame().is_some();
    for mut camera in &mut cameras {
        if camera.is_active != active {
            camera.is_active = active;
        }
    }
    let visibility = if active {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut quad in &mut quads {
        quad.set_if_neq(visibility);
    }
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn sync_camera(
    game: Res<Game>,
    time: Res<Time>,
    window: Single<&Window>,
    mut state: ResMut<CameraState>,
    mut composite: Single<&mut Projection, With<CompositeCamera>>,
    mut world_camera: Single<
        &mut Transform,
        (
            With<WorldCamera>,
            Without<EmissiveCamera>,
            Without<LayerQuad>,
            Without<PassQuad>,
        ),
    >,
    mut emissive_camera: Single<
        &mut Transform,
        (
            With<EmissiveCamera>,
            Without<WorldCamera>,
            Without<LayerQuad>,
            Without<PassQuad>,
        ),
    >,
    mut quads: Query<
        (&LayerQuad, &mut Transform),
        (
            Without<WorldCamera>,
            Without<EmissiveCamera>,
            Without<PassQuad>,
        ),
    >,
    mut pass_quads: Query<
        (&PassQuad, &mut Transform),
        (
            Without<WorldCamera>,
            Without<EmissiveCamera>,
            Without<LayerQuad>,
        ),
    >,
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
    emissive_camera.translation.x = snapped.x as f32;
    emissive_camera.translation.y = snapped.y as f32;

    for (pass, mut transform) in &mut pass_quads {
        let size = target_size(pass.0, state.native).as_vec2();
        let scale = Vec3::new(size.x, size.y, 1.0);
        if transform.scale != scale {
            transform.scale = scale;
        }
    }

    let calendar = game
        .0
        .ingame()
        .map(|ingame| ingame.clock.calendar)
        .unwrap_or_default();
    state.star_scroll = star_scroll(calendar);
    let size = (state.native * state.k).as_vec2();
    for (quad, mut transform) in &mut quads {
        let (_, remainder_px) = state.layer(quad.ratio);
        let drift_px = if quad.drift {
            (state.star_scroll - state.star_scroll.floor()) * state.k as f32
        } else {
            Vec2::ZERO
        };
        let raw = remainder_px + drift_px;
        let offset = match game.0.settings.render_mode {
            RenderMode::PixelPerfect => -raw.round(),
            RenderMode::Smooth => -raw,
            RenderMode::Retro => Vec2::ZERO,
        };
        transform.translation = offset.extend(quad.z);
        transform.scale = Vec3::new(size.x, size.y, 1.0);
    }
}

pub fn resize_targets(
    state: Res<CameraState>,
    mut images: ResMut<Assets<Image>>,
    mut targets: ResMut<LayerTargets>,
    mut cameras: Query<(&LayerCamera, &mut Projection, &mut RenderTarget)>,
) {
    if targets.native == state.native {
        return;
    }
    targets.native = state.native;

    for (layer, mut projection, mut target) in &mut cameras {
        let size = target_size(layer.0, state.native);
        let handle = native_target(&mut images, size, target_sampler(layer.0));
        targets.handles[layer.0] = handle.clone();
        *projection = fixed_projection(size);
        *target = RenderTarget::from(handle);
    }
}

pub fn rebind_targets(
    targets: Res<LayerTargets>,
    assets: Res<LayerAssets>,
    mut upscale_mats: ResMut<Assets<UpscaleMaterial>>,
    mut lighting_mats: ResMut<Assets<LightingMaterial>>,
    mut downsample_mats: ResMut<Assets<DownsampleMaterial>>,
    mut light_blur_mats: ResMut<Assets<LightBlurMaterial>>,
) {
    if !targets.is_changed() {
        return;
    }
    if let Some(mut material) = lighting_mats.get_mut(&assets.lighting) {
        material.world = targets.handles[L_WORLD].clone();
        material.light = targets.handles[L_LIGHT].clone();
        material.emission = targets.handles[L_EMISSIVE_SRC].clone();
    }
    if let Some(mut material) = downsample_mats.get_mut(&assets.down_half) {
        material.src = targets.handles[L_EMISSIVE_SRC].clone();
    }
    if let Some(mut material) = downsample_mats.get_mut(&assets.down_quarter) {
        material.src = targets.handles[L_LIGHT_HALF].clone();
    }
    if let Some(mut material) = light_blur_mats.get_mut(&assets.light_blur_h) {
        material.src = targets.handles[L_LIGHT_QUARTER].clone();
    }
    if let Some(mut material) = light_blur_mats.get_mut(&assets.light_blur_v) {
        material.src = targets.handles[L_LIGHT_TMP].clone();
    }
    for (i, handle) in assets.upscale.iter().enumerate() {
        if let Some(handle) = handle
            && let Some(mut material) = upscale_mats.get_mut(handle)
        {
            material.texture = targets.handles[i].clone();
        }
    }
}

pub fn cursor_to_world(window: &Window, state: &CameraState) -> Option<Vec2> {
    let cursor = window.cursor_position()? * window.scale_factor();
    let centered = cursor - state.window_px.as_vec2() / 2.0;
    Some(state.pos + Vec2::new(centered.x, -centered.y) / state.k as f32)
}
