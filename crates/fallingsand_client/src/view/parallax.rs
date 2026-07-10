use super::camera::{
    CameraState, FAR_LAYER, FAR_RATIO, L_FAR, L_NEAR, L_WALL, LayerCamera, NEAR_LAYER, NEAR_RATIO,
    WALL_LAYER, WALL_RATIO, layer_camera,
};
use super::sky::{ActiveLights, LightingParams, Sky, sky_color};
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d};

const WALL_COLOR: Vec3 = Vec3::new(0.060, 0.052, 0.048);
const FAR_HAZE: f32 = 0.6;
const NEAR_HAZE: f32 = 0.35;
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
pub struct ParallaxAssets {
    wall: Handle<CaveWallMaterial>,
    far: Handle<SilhouetteMaterial>,
    near: Handle<SilhouetteMaterial>,
}

#[derive(Component)]
pub(crate) struct ParallaxSource;

pub fn setup_parallax(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut wall_mats: ResMut<Assets<CaveWallMaterial>>,
    mut silhouette_mats: ResMut<Assets<SilhouetteMaterial>>,
    state: Res<CameraState>,
    cameras: Query<(Entity, &LayerCamera)>,
) {
    let (Some(far_cam), Some(near_cam), Some(wall_cam)) = (
        layer_camera(&cameras, L_FAR),
        layer_camera(&cameras, L_NEAR),
        layer_camera(&cameras, L_WALL),
    ) else {
        return;
    };
    let quad = meshes.add(Rectangle::default());
    let native = state.native.as_vec2().extend(1.0);
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

    for (camera, material, layer) in [
        (far_cam, MeshMaterial2d(far.clone()), FAR_LAYER),
        (near_cam, MeshMaterial2d(near.clone()), NEAR_LAYER),
    ] {
        commands.entity(camera).with_children(|parent| {
            parent.spawn((
                ParallaxSource,
                Mesh2d(quad.clone()),
                material,
                Transform::from_scale(native),
                RenderLayers::layer(layer),
                Visibility::Hidden,
            ));
        });
    }
    commands.entity(wall_cam).with_children(|parent| {
        parent.spawn((
            ParallaxSource,
            Mesh2d(quad.clone()),
            MeshMaterial2d(wall.clone()),
            Transform::from_scale(native),
            RenderLayers::layer(WALL_LAYER),
            Visibility::Hidden,
        ));
    });

    commands.insert_resource(ParallaxAssets { wall, far, near });
}

pub fn sync_parallax(
    sky: Res<Sky>,
    state: Res<CameraState>,
    active: Res<ActiveLights>,
    assets: Res<ParallaxAssets>,
    mut wall_mats: ResMut<Assets<CaveWallMaterial>>,
    mut silhouette_mats: ResMut<Assets<SilhouetteMaterial>>,
    mut sources: Query<(&mut Transform, &mut Visibility), With<ParallaxSource>>,
) {
    let native = state.native.as_vec2();
    for (mut transform, mut visibility) in &mut sources {
        transform.scale = native.extend(1.0);
        *visibility = if sky.synced {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if !sky.synced {
        return;
    }

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

    for (handle, ratio, haze) in [
        (&assets.far, FAR_RATIO, FAR_HAZE),
        (&assets.near, NEAR_RATIO, NEAR_HAZE),
    ] {
        if let Some(mut material) = silhouette_mats.get_mut(handle) {
            let rgb = sky_linear * haze;
            material.params.color = rgb.extend(1.0);
            let (snapped, _) = state.layer(ratio);
            material.params.snapped_cam = snapped.as_vec2();
            material.params.native_size = native;
        }
    }
}
