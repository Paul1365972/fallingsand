use crate::camera::{CameraSet, CameraState, CompositeCamera, LayerQuad};
use crate::sky::{ActiveLights, LightSet, LightingParams, Sky, sky_color};
use crate::{AppState, GameState};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use fallingsand_core::smoothstep;

pub struct ParallaxPlugin;

const WALL_RATIO: Vec2 = Vec2::splat(0.15);
const FAR_RATIO: Vec2 = Vec2::new(0.88, 0.92);
const NEAR_RATIO: Vec2 = Vec2::new(0.72, 0.80);
const WALL_COLOR: Vec3 = Vec3::new(0.060, 0.052, 0.048);
const FAR_BASE: f32 = 14.0;
const FAR_AMP: f32 = 90.0;
const FAR_WAVELENGTH: f32 = 220.0;
const NEAR_BASE: f32 = 4.0;
const NEAR_AMP: f32 = 45.0;
const NEAR_WAVELENGTH: f32 = 90.0;

#[derive(ShaderType, Debug, Clone, Default)]
pub struct WallParams {
    pub base_color: Vec4,
    pub world_offset: Vec2,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct CaveWallMaterial {
    #[uniform(0)]
    pub lighting: LightingParams,
    #[uniform(1)]
    pub wall: WallParams,
}

impl Material2d for CaveWallMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/cave_wall.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct SilhouetteParams {
    pub color: Vec4,
    pub snapped_cam: Vec2,
    pub native_size: Vec2,
    pub base: f32,
    pub amp: f32,
    pub inv_wavelength: f32,
    pub seed: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct SilhouetteMaterial {
    #[uniform(0)]
    pub params: SilhouetteParams,
}

impl Material2d for SilhouetteMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/silhouette.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Resource)]
struct ParallaxAssets {
    wall: Handle<CaveWallMaterial>,
    far: Handle<SilhouetteMaterial>,
    near: Handle<SilhouetteMaterial>,
}

#[derive(Component)]
struct ParallaxQuad;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<CaveWallMaterial>::default())
            .add_plugins(Material2dPlugin::<SilhouetteMaterial>::default())
            .add_systems(PostStartup, setup_parallax)
            .add_systems(
                Update,
                update_parallax
                    .after(CameraSet::Derive)
                    .after(LightSet)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(OnExit(AppState::InGame), hide_parallax);
    }
}

fn setup_parallax(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut wall_mats: ResMut<Assets<CaveWallMaterial>>,
    mut silhouette_mats: ResMut<Assets<SilhouetteMaterial>>,
    composite: Single<Entity, With<CompositeCamera>>,
) {
    let quad = meshes.add(Rectangle::default());
    let wall = wall_mats.add(CaveWallMaterial {
        wall: WallParams {
            base_color: WALL_COLOR.extend(1.0),
            world_offset: Vec2::ZERO,
        },
        ..default()
    });
    let far = silhouette_mats.add(SilhouetteMaterial {
        params: SilhouetteParams {
            base: FAR_BASE,
            amp: FAR_AMP,
            inv_wavelength: 1.0 / FAR_WAVELENGTH,
            seed: 17.0,
            ..default()
        },
    });
    let near = silhouette_mats.add(SilhouetteMaterial {
        params: SilhouetteParams {
            base: NEAR_BASE,
            amp: NEAR_AMP,
            inv_wavelength: 1.0 / NEAR_WAVELENGTH,
            seed: 53.0,
            ..default()
        },
    });

    commands.entity(*composite).with_children(|parent| {
        parent.spawn((
            ParallaxQuad,
            LayerQuad {
                ratio: FAR_RATIO,
                z: -40.0,
            },
            Mesh2d(quad.clone()),
            MeshMaterial2d(far.clone()),
            Transform::from_xyz(0.0, 0.0, -40.0),
            Visibility::Hidden,
        ));
        parent.spawn((
            ParallaxQuad,
            LayerQuad {
                ratio: NEAR_RATIO,
                z: -38.0,
            },
            Mesh2d(quad.clone()),
            MeshMaterial2d(near.clone()),
            Transform::from_xyz(0.0, 0.0, -38.0),
            Visibility::Hidden,
        ));
        parent.spawn((
            ParallaxQuad,
            LayerQuad {
                ratio: WALL_RATIO,
                z: -20.0,
            },
            Mesh2d(quad),
            MeshMaterial2d(wall.clone()),
            Transform::from_xyz(0.0, 0.0, -20.0),
            Visibility::Hidden,
        ));
    });
    commands.insert_resource(ParallaxAssets { wall, far, near });
}

fn altitude_fade(pos_y: f32) -> f32 {
    (1.0 - smoothstep(350.0, 900.0, pos_y)) * (1.0 - smoothstep(60.0, 220.0, -pos_y))
}

#[allow(clippy::too_many_arguments)]
fn update_parallax(
    sky: Res<Sky>,
    state: Res<CameraState>,
    active: Res<ActiveLights>,
    assets: Res<ParallaxAssets>,
    mut wall_mats: ResMut<Assets<CaveWallMaterial>>,
    mut silhouette_mats: ResMut<Assets<SilhouetteMaterial>>,
    mut quads: Query<&mut Visibility, With<ParallaxQuad>>,
) {
    for mut visibility in &mut quads {
        *visibility = if sky.synced {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if !sky.synced {
        return;
    }

    let native = state.native.as_vec2();
    if let Some(mut material) = wall_mats.get_mut(&assets.wall) {
        active.write(&mut material.lighting);
        let (snapped, _) = state.layer(WALL_RATIO);
        material.lighting.snapped_cam = snapped.as_vec2();
        material.lighting.native_size = native;
        material.wall.world_offset = WALL_RATIO * state.pos;
    }

    let srgb = sky_color(
        sky.state.light,
        sky.state.sun_altitude,
        sky.state.solar_occlusion,
    );
    let linear = Color::srgb(srgb.x, srgb.y, srgb.z).to_linear();
    let sky_linear = Vec3::new(linear.red, linear.green, linear.blue);
    let deep = Vec3::new(0.04, 0.05, 0.09);
    let dim = 1.0 - (1.0 - sky.state.light) * 0.85;
    let fade = altitude_fade(state.pos.y);

    for (handle, ratio, mix) in [
        (&assets.far, FAR_RATIO, 0.35),
        (&assets.near, NEAR_RATIO, 0.6),
    ] {
        if let Some(mut material) = silhouette_mats.get_mut(handle) {
            let rgb = sky_linear.lerp(deep, mix) * dim;
            material.params.color = rgb.extend(fade);
            let (snapped, _) = state.layer(ratio);
            material.params.snapped_cam = snapped.as_vec2();
            material.params.native_size = native;
        }
    }
}

fn hide_parallax(mut quads: Query<&mut Visibility, With<ParallaxQuad>>) {
    for mut visibility in &mut quads {
        *visibility = Visibility::Hidden;
    }
}
