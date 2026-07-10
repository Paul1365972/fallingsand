use crate::input::LocalAction;
use crate::net::Session;
use crate::player::PlayerVisual;
use crate::{AppState, PauseState};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Hdr, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::Image;
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::ui::IsDefaultUiCamera;

pub struct CameraPlugin;

pub const VIRTUAL_WIDTH: f32 = 424.0;
pub const WORLD_LAYER: usize = 1;
pub const SKY_LAYER: usize = 2;

#[derive(Component)]
pub struct WorldCamera;

#[derive(Component)]
pub struct SkyLayerCamera;

#[derive(Component)]
pub struct CompositeCamera;

#[derive(Resource)]
pub struct WorldTarget(pub Handle<Image>);

#[derive(Resource)]
pub struct SkyTarget(pub Handle<Image>);

#[derive(Component)]
pub struct LayerQuad {
    pub ratio: Vec2,
    pub z: f32,
}

#[derive(Resource, Default)]
pub struct CameraControl {
    pub zoom_index: i32,
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    #[default]
    PixelPerfect,
    Smooth,
    Retro,
}

impl RenderMode {
    pub fn label(self) -> &'static str {
        match self {
            RenderMode::PixelPerfect => "pixel-perfect",
            RenderMode::Smooth => "smooth",
            RenderMode::Retro => "retro",
        }
    }
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

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraSet {
    Scale,
    Follow,
    Derive,
}

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraControl>()
            .init_resource::<CameraState>()
            .init_resource::<RenderMode>()
            .add_systems(Startup, setup_camera)
            .configure_sets(
                Update,
                (CameraSet::Scale, CameraSet::Follow, CameraSet::Derive).chain(),
            )
            .add_systems(
                Update,
                (
                    camera_input
                        .run_if(in_state(PauseState::Running))
                        .before(CameraSet::Scale),
                    update_pixel_scale.in_set(CameraSet::Scale),
                    follow_player.in_set(CameraSet::Follow),
                    derive_layers.in_set(CameraSet::Derive),
                ),
            )
            .add_systems(OnExit(AppState::InGame), reset_camera);
    }
}

fn base_scale(window_px: UVec2) -> u32 {
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

fn setup_camera(
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

    let world_target = native_target(&mut images, native);
    let sky_target = native_target(&mut images, native);
    commands.insert_resource(WorldTarget(world_target.clone()));
    commands.insert_resource(SkyTarget(sky_target.clone()));

    commands.spawn((
        Camera2d,
        Hdr,
        Msaa::Off,
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::from(sky_target),
        RenderLayers::layer(SKY_LAYER),
        fixed_projection(native),
        Transform::IDENTITY,
        SkyLayerCamera,
    ));

    commands.spawn((
        Camera2d,
        Hdr,
        Msaa::Off,
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::from(world_target),
        RenderLayers::layer(WORLD_LAYER),
        fixed_projection(native),
        Transform::IDENTITY,
        WorldCamera,
    ));

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

fn camera_input(
    mut actions: MessageReader<LocalAction>,
    mut control: ResMut<CameraControl>,
    mut mode: ResMut<RenderMode>,
) {
    for action in actions.read() {
        match action {
            LocalAction::Zoom(scroll) => {
                control.zoom_index += scroll.signum() as i32;
            }
            LocalAction::CycleRenderMode => {
                *mode = match *mode {
                    RenderMode::PixelPerfect => RenderMode::Smooth,
                    RenderMode::Smooth => RenderMode::Retro,
                    RenderMode::Retro => RenderMode::PixelPerfect,
                };
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn update_pixel_scale(
    window: Single<&Window>,
    mut control: ResMut<CameraControl>,
    mut state: ResMut<CameraState>,
    mut images: ResMut<Assets<Image>>,
    world_target: Res<WorldTarget>,
    sky_target: Res<SkyTarget>,
    mut natives: Query<
        &mut Projection,
        (
            Or<(With<WorldCamera>, With<SkyLayerCamera>)>,
            Without<CompositeCamera>,
        ),
    >,
    mut composite: Single<&mut Projection, With<CompositeCamera>>,
) {
    let window_px = UVec2::new(
        window.physical_width().max(1),
        window.physical_height().max(1),
    );
    let (k, native) = pixel_scale(window_px, control.zoom_index);
    let clamped_index = k as i32 - base_scale(window_px) as i32;
    if control.zoom_index != clamped_index {
        control.zoom_index = clamped_index;
    }
    if state.k == k && state.native == native && state.window_px == window_px {
        return;
    }
    let resize_native = state.native != native;
    let resize_window = state.window_px != window_px;
    state.k = k;
    state.native = native;
    state.window_px = window_px;

    if resize_native {
        for target in [&world_target.0, &sky_target.0] {
            if let Some(mut image) = images.get_mut(target) {
                image.resize(Extent3d {
                    width: native.x,
                    height: native.y,
                    depth_or_array_layers: 1,
                });
            }
        }
        for mut projection in &mut natives {
            *projection = fixed_projection(native);
        }
    }
    if resize_window {
        **composite = fixed_projection(window_px);
    }
}

fn follow_player(
    time: Res<Time>,
    session: Option<Res<Session>>,
    players: Query<(&PlayerVisual, &Transform)>,
    mut state: ResMut<CameraState>,
) {
    let target = if let Some(id) = session.and_then(|session| session.player)
        && let Some((_, transform)) = players.iter().find(|(visual, _)| visual.id == id)
    {
        transform.translation.truncate()
    } else {
        return;
    };
    let blend = 1.0 - (-8.0 * time.delta_secs()).exp();
    state.pos = state.pos.lerp(target, blend);
}

#[allow(clippy::type_complexity)]
fn derive_layers(
    state: Res<CameraState>,
    mode: Res<RenderMode>,
    mut world_camera: Single<&mut Transform, (With<WorldCamera>, Without<LayerQuad>)>,
    mut quads: Query<(&LayerQuad, &mut Transform), Without<WorldCamera>>,
) {
    let (snapped, _) = state.layer(Vec2::ZERO);
    world_camera.translation.x = snapped.x as f32;
    world_camera.translation.y = snapped.y as f32;

    let size = (state.native * state.k).as_vec2();
    for (quad, mut transform) in &mut quads {
        let (_, remainder_px) = state.layer(quad.ratio);
        let offset = match *mode {
            RenderMode::PixelPerfect => -remainder_px.round(),
            RenderMode::Smooth => -remainder_px,
            RenderMode::Retro => Vec2::ZERO,
        };
        transform.translation = offset.extend(quad.z);
        transform.scale = Vec3::new(size.x, size.y, 1.0);
    }
}

pub fn cursor_to_world(window: &Window, state: &CameraState) -> Option<Vec2> {
    let cursor = window.cursor_position()? * window.scale_factor();
    let centered = cursor - state.window_px.as_vec2() / 2.0;
    Some(state.pos + Vec2::new(centered.x, -centered.y) / state.k as f32)
}

fn reset_camera(mut control: ResMut<CameraControl>, mut state: ResMut<CameraState>) {
    *control = CameraControl::default();
    state.pos = CameraState::default().pos;
}
