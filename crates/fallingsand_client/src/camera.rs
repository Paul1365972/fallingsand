use crate::net::Session;
use crate::player::PlayerVisual;
use crate::{AppState, PauseState};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Hdr, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::Image;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::ui::IsDefaultUiCamera;

pub struct CameraPlugin;

pub const VIRTUAL_WIDTH: f32 = 424.0;
pub const VIRTUAL_HEIGHT: f32 = 242.0;
pub const WORLD_LAYER: usize = 1;
const ZOOM_STEPS: i32 = 4;

#[derive(Component)]
pub struct GameCamera;

#[derive(Component)]
pub struct WorldCamera;

#[derive(Component)]
pub struct SkyCamera;

#[derive(Resource)]
pub struct WorldTarget(pub Handle<Image>);

#[derive(Resource)]
pub struct CameraControl {
    pub zoom: f32,
    scroll: f32,
}

impl Default for CameraControl {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            scroll: 0.0,
        }
    }
}

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraControl>()
            .add_systems(Startup, setup_camera)
            .add_systems(Update, resize_world_target)
            .add_systems(
                Update,
                (
                    zoom_input.run_if(in_state(PauseState::Running)),
                    follow_player.after(crate::interpolation::interpolate),
                )
                    .chain(),
            )
            .add_systems(OnExit(AppState::InGame), reset_camera);
    }
}

fn reset_camera(
    mut control: ResMut<CameraControl>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    *control = CameraControl::default();
    for (mut transform, mut projection) in &mut cameras {
        transform.translation = Vec3::new(0.0, 24.0, 0.0);
        if let Projection::Orthographic(ortho) = &mut *projection {
            ortho.scale = 1.0;
        }
    }
}

fn ortho() -> Projection {
    Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::AutoMin {
            min_width: VIRTUAL_WIDTH,
            min_height: VIRTUAL_HEIGHT,
        },
        ..OrthographicProjection::default_2d()
    })
}

fn setup_camera(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    window: Single<&Window>,
) {
    let size = Extent3d {
        width: window.physical_width().max(1),
        height: window.physical_height().max(1),
        depth_or_array_layers: 1,
    };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0; 8],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let target = images.add(image);
    commands.insert_resource(WorldTarget(target.clone()));

    commands.spawn((
        Camera2d,
        Hdr,
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::from(target),
        RenderLayers::layer(WORLD_LAYER),
        ortho(),
        Transform::from_xyz(0.0, 24.0, 0.0),
        GameCamera,
        WorldCamera,
    ));

    commands.spawn((
        Camera2d,
        Hdr,
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
        ortho(),
        Transform::from_xyz(0.0, 24.0, 0.0),
        GameCamera,
        SkyCamera,
    ));
}

fn resize_world_target(
    target: Res<WorldTarget>,
    mut images: ResMut<Assets<Image>>,
    window: Single<&Window>,
) {
    let width = window.physical_width().max(1);
    let height = window.physical_height().max(1);
    if let Some(mut image) = images.get_mut(&target.0)
        && (image.width() != width || image.height() != height)
    {
        image.resize(Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        });
    }
}

fn zoom_input(
    mut wheel: MessageReader<MouseWheel>,
    mut control: ResMut<CameraControl>,
    mut projections: Query<&mut Projection, With<GameCamera>>,
) {
    let scroll: f32 = wheel
        .read()
        .map(|event| match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / 60.0,
        })
        .sum();
    if scroll != 0.0 {
        let range = ZOOM_STEPS as f32;
        control.scroll = (control.scroll - scroll).clamp(-range, range);
        control.zoom = 2f32.powf(control.scroll.round() / range);
    }
    for mut projection in &mut projections {
        if let Projection::Orthographic(ortho) = &mut *projection {
            ortho.scale = control.zoom;
        }
    }
}

fn follow_player(
    time: Res<Time>,
    session: Option<Res<Session>>,
    players: Query<(&PlayerVisual, &Transform), Without<GameCamera>>,
    mut cameras: Query<&mut Transform, With<GameCamera>>,
) {
    let target = if let Some(id) = session.and_then(|session| session.player)
        && let Some((_, transform)) = players.iter().find(|(visual, _)| visual.id == id)
    {
        transform.translation.truncate()
    } else {
        return;
    };
    let blend = 1.0 - (-8.0 * time.delta_secs()).exp();
    for mut camera in &mut cameras {
        let current = camera.translation.truncate();
        let next = current.lerp(target, blend);
        camera.translation.x = next.x;
        camera.translation.y = next.y;
    }
}
