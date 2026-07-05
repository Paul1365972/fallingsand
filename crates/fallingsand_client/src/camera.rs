use crate::net::Session;
use crate::player::PlayerVisual;
use crate::{AppState, PauseState};
use bevy::camera::ScalingMode;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

pub struct CameraPlugin;

pub const VIRTUAL_WIDTH: f32 = 424.0;
pub const VIRTUAL_HEIGHT: f32 = 242.0;
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 2.0;

#[derive(Resource)]
pub struct CameraControl {
    pub zoom: f32,
    pub free_pan: Option<Vec2>,
}

impl Default for CameraControl {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            free_pan: None,
        }
    }
}

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraControl>()
            .add_systems(Startup, setup_camera)
            .add_systems(
                Update,
                (
                    zoom_input.run_if(in_state(PauseState::Running)),
                    pan_input.run_if(in_state(PauseState::Running)),
                    follow_player.after(crate::interpolation::interpolate),
                )
                    .chain(),
            )
            .add_systems(OnExit(AppState::InGame), reset_camera);
    }
}

fn reset_camera(
    mut control: ResMut<CameraControl>,
    mut camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    *control = CameraControl::default();
    let (transform, projection) = &mut *camera;
    transform.translation = Vec3::new(0.0, 24.0, 0.0);
    if let Projection::Orthographic(ortho) = &mut **projection {
        ortho.scale = 1.0;
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: VIRTUAL_WIDTH,
                min_height: VIRTUAL_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(0.0, 24.0, 0.0),
    ));
}

fn zoom_input(
    mut wheel: MessageReader<MouseWheel>,
    mut control: ResMut<CameraControl>,
    mut projection: Single<&mut Projection, With<Camera2d>>,
) {
    let mut scroll = 0.0f32;
    for event in wheel.read() {
        scroll += match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / 60.0,
        };
    }
    if scroll != 0.0 {
        control.zoom = (control.zoom * 0.9f32.powf(scroll)).clamp(MIN_ZOOM, MAX_ZOOM);
    }
    if let Projection::Orthographic(ortho) = &mut **projection {
        ortho.scale = control.zoom;
    }
}

fn pan_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut control: ResMut<CameraControl>,
    camera: Single<&Transform, With<Camera2d>>,
) {
    let mut direction = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyJ) {
        direction.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyL) {
        direction.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyI) {
        direction.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyK) {
        direction.y -= 1.0;
    }
    if direction != Vec2::ZERO {
        let base = control
            .free_pan
            .unwrap_or_else(|| camera.translation.truncate());
        control.free_pan =
            Some(base + direction.normalize() * 300.0 * control.zoom * time.delta_secs());
    }
    if keys.just_pressed(KeyCode::KeyO) {
        control.free_pan = None;
    }
}

fn follow_player(
    time: Res<Time>,
    control: Res<CameraControl>,
    session: Option<Res<Session>>,
    players: Query<(&PlayerVisual, &Transform), Without<Camera2d>>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
) {
    let target = if let Some(pan) = control.free_pan {
        pan
    } else if let Some(id) = session.and_then(|session| session.player)
        && let Some((_, transform)) = players.iter().find(|(visual, _)| visual.id == id)
    {
        transform.translation.truncate()
    } else {
        return;
    };
    let blend = 1.0 - (-8.0 * time.delta_secs()).exp();
    let current = camera.translation.truncate();
    let next = current.lerp(target, blend);
    camera.translation.x = next.x;
    camera.translation.y = next.y;
}
