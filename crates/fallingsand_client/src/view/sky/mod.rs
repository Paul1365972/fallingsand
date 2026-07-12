mod lighting;
mod materials;

pub use lighting::{ActiveLights, EmissiveLights, apply_lighting, scan_emissive};
pub use materials::{HorizonMaterial, MoonMaterial, StarfieldMaterial, SunMaterial};
pub use materials::{LightingMaterial, LightingParams};

use super::Game;
use super::camera::{
    CameraState, CompositeCamera, L_SKY, L_STAR, LayerCamera, SKY_LAYER, STAR_LAYER,
    STAR_WORLD_TILE, layer_camera,
};
use crate::game::RenderMode;
use bevy::camera::visibility::RenderLayers;
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy::shader::Shader;
use fallingsand_core::celestial::SHADE_DISC_RADIUS;
use fallingsand_core::{CelestialState, smoothstep};
use materials::{MoonParams, StarfieldParams, SunParams};

const MAX_DARKNESS: f32 = 0.82;
const HORIZON_FRAC: f32 = 0.22;
const ORBIT_RADIUS: f32 = 133.0;

const SUN_SIZE: f32 = 48.0;
const MOON_SIZE: f32 = 28.0;

const DEFAULT_CLEAR: Color = Color::srgb(0.08, 0.09, 0.13);

#[derive(Resource, Default, Clone, Copy)]
pub struct Sky {
    pub state: CelestialState,
    pub synced: bool,
    pub color_linear: Vec3,
}

impl Sky {
    pub fn darkness(&self) -> f32 {
        (1.0 - self.state.light) * MAX_DARKNESS
    }
}

#[derive(Resource)]
pub struct SkyAssets {
    sun: Handle<SunMaterial>,
    moon: Handle<MoonMaterial>,
    starfield: Handle<StarfieldMaterial>,
    horizon: Handle<HorizonMaterial>,
}

#[derive(Resource)]
pub(crate) struct SharedShaders(#[allow(dead_code)] Vec<Handle<Shader>>);

#[derive(Component)]
pub(crate) struct StarfieldQuad;

#[derive(Component)]
pub(crate) struct HorizonQuad;

#[derive(Component)]
pub(crate) struct SunVisual;

#[derive(Component)]
pub(crate) struct MoonVisual;

pub fn load_shared_shaders(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(SharedShaders(vec![
        asset_server.load("shaders/layer_common.wgsl"),
        asset_server.load("shaders/light_common.wgsl"),
    ]));
}

#[allow(clippy::too_many_arguments)]
pub fn setup_sky(
    mut commands: Commands,
    mut sun_mats: ResMut<Assets<SunMaterial>>,
    mut moon_mats: ResMut<Assets<MoonMaterial>>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    mut horizon_mats: ResMut<Assets<HorizonMaterial>>,
    asset_server: Res<AssetServer>,
    shared: Res<super::chunks::RenderShared>,
    cameras: Query<(Entity, &LayerCamera)>,
    composite: Single<Entity, With<CompositeCamera>>,
) {
    let sky_camera = layer_camera(&cameras, L_SKY).expect("layer cameras exist");
    let star_camera = layer_camera(&cameras, L_STAR).expect("layer cameras exist");
    let composite_camera = *composite;
    let quad = shared.quad.clone();
    let sun = sun_mats.add(SunMaterial {
        params: SunParams::default(),
        texture: asset_server.load("sky/sun.png"),
    });
    let moon = moon_mats.add(MoonMaterial {
        params: MoonParams::default(),
        texture: asset_server.load("sky/moon.png"),
    });
    let starfield = star_mats.add(StarfieldMaterial {
        params: StarfieldParams::default(),
        texture: asset_server
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    mag_filter: ImageFilterMode::Nearest,
                    min_filter: ImageFilterMode::Nearest,
                    ..default()
                });
            })
            .load("sky/stars.png"),
    });
    let horizon = horizon_mats.add(HorizonMaterial::default());

    commands.entity(star_camera).with_children(|parent| {
        parent.spawn((
            StarfieldQuad,
            Mesh2d(quad.clone()),
            MeshMaterial2d(starfield.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0),
            RenderLayers::layer(STAR_LAYER),
            Visibility::Hidden,
        ));
    });
    commands.entity(sky_camera).with_children(|parent| {
        parent.spawn((
            HorizonQuad,
            Mesh2d(quad.clone()),
            MeshMaterial2d(horizon.clone()),
            Transform::from_xyz(0.0, 0.0, -45.0),
            RenderLayers::layer(SKY_LAYER),
            Visibility::Hidden,
        ));
    });
    commands.entity(composite_camera).with_children(|parent| {
        parent.spawn((
            SunVisual,
            Mesh2d(quad.clone()),
            MeshMaterial2d(sun.clone()),
            Transform::from_xyz(0.0, -1000.0, -45.5),
        ));
        parent.spawn((
            MoonVisual,
            Mesh2d(quad.clone()),
            MeshMaterial2d(moon.clone()),
            Transform::from_xyz(0.0, -1000.0, -45.0),
        ));
    });
    commands.insert_resource(SkyAssets {
        sun,
        moon,
        starfield,
        horizon,
    });
}

pub fn sky_color(light: f32, sun_alt: f32, solar_occ: f32) -> Vec3 {
    let night = Vec3::new(0.015, 0.025, 0.055);
    let day = Vec3::new(0.40, 0.60, 0.86);
    let horizon = Vec3::new(0.85, 0.45, 0.28);
    let base = night.lerp(day, light);
    let band = (1.0 - sun_alt.abs()).powi(3);
    let warm = band * (1.0 - solar_occ) * 0.6;
    let mut rgb = base.lerp(horizon, warm);
    if solar_occ > 0.0 {
        let grey = Vec3::splat((rgb.x + rgb.y + rgb.z) / 3.0);
        rgb = rgb.lerp(grey, solar_occ * 0.4);
    }
    rgb
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn sync_sky(
    game: Res<Game>,
    state: Res<CameraState>,
    assets: Res<SkyAssets>,
    mut sky: ResMut<Sky>,
    mut clear: ResMut<ClearColor>,
    mut sun_mats: ResMut<Assets<SunMaterial>>,
    mut moon_mats: ResMut<Assets<MoonMaterial>>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    mut horizon_mats: ResMut<Assets<HorizonMaterial>>,
    mut quads: ParamSet<(
        Query<(&mut Transform, &mut Visibility), With<SunVisual>>,
        Query<(&mut Transform, &mut Visibility), With<MoonVisual>>,
        Query<(&mut Transform, &mut Visibility), With<StarfieldQuad>>,
        Query<(&mut Transform, &mut Visibility), With<HorizonQuad>>,
    )>,
) {
    let clock = game.0.ingame().map(|ingame| ingame.clock);
    let synced = clock.is_some_and(|clock| clock.synced);
    let calendar = clock.map(|clock| clock.calendar).unwrap_or_default();
    sky.synced = synced;

    let center = Vec2::new(0.0, -HORIZON_FRAC * ORBIT_RADIUS);
    let native = state.native.as_vec2();
    let horizon_uv = 0.5 + HORIZON_FRAC * ORBIT_RADIUS / native.y;

    if synced {
        let celestial = calendar.celestial();

        let sun_position = Vec2::from(celestial.sun_position) * ORBIT_RADIUS;
        let moon_position = Vec2::from(celestial.moon_position) * ORBIT_RADIUS;
        let shade_position = Vec2::from(celestial.shade_position) * ORBIT_RADIUS;
        let sun_altitude = celestial.sun_altitude;
        let solar_occlusion = celestial.solar_occlusion;

        let moon_size = (MOON_SIZE * celestial.moon_radius_scale).round().max(1.0);
        let world_to_moon_uv = 2.0 / moon_size;
        let umbra = (shade_position - moon_position) * world_to_moon_uv;
        let umbra_radius = SHADE_DISC_RADIUS * ORBIT_RADIUS * world_to_moon_uv;

        let color = sky_color(celestial.light, sun_altitude, solar_occlusion);
        let linear = Color::srgb(color.x, color.y, color.z).to_linear();
        sky.state = celestial;
        sky.color_linear = Vec3::new(linear.red, linear.green, linear.blue);

        let k = state.k as f32;
        let place = |p: Vec2| match game.0.settings.render_mode {
            RenderMode::PixelPerfect => (p * k).round(),
            RenderMode::Smooth => p * k,
            RenderMode::Retro => p.round() * k,
        };

        if let Ok((mut transform, mut visibility)) = quads.p0().single_mut() {
            transform.translation = place(sun_position + center).extend(-45.5);
            transform.scale = Vec3::new(SUN_SIZE * k, SUN_SIZE * k, 1.0);
            *visibility = Visibility::Inherited;
        }
        if let Some(mut material) = sun_mats.get_mut(&assets.sun) {
            material.params.redness = 1.0 - smoothstep(0.0, 0.35, sun_altitude);
            material.params.occlusion = solar_occlusion;
        }

        if let Ok((mut transform, mut visibility)) = quads.p1().single_mut() {
            transform.translation = place(moon_position + center).extend(-45.0);
            transform.scale = Vec3::new(moon_size * k, moon_size * k, 1.0);
            *visibility = Visibility::Inherited;
        }
        if let Some(mut material) = moon_mats.get_mut(&assets.moon) {
            material.params.sun_direction = Vec2::from(celestial.sun_direction);
            material.params.illumination = celestial.illumination;
            material.params.umbra = umbra;
            material.params.umbra_radius = umbra_radius;
            material.params.sky_color = sky.color_linear.extend(celestial.daylight);
            material.params.quad_size = moon_size;
        }

        clear.0 = Color::srgb(color.x, color.y, color.z);

        if let Some(mut material) = star_mats.get_mut(&assets.starfield) {
            material.params.center = center;
            material.params.native_size = native;
            material.params.scroll = state.star_scroll.floor();
            material.params.world_scale = STAR_WORLD_TILE;
            material.params.star_visibility = sky.state.star_visibility;
            material.params.horizon = horizon_uv;
            material.params.sidereal = calendar.sidereal();
        }
        if let Some(mut material) = horizon_mats.get_mut(&assets.horizon) {
            let day_haze = Vec3::new(0.72, 0.82, 0.96);
            let night_haze = Vec3::new(0.08, 0.11, 0.20);
            let warm = Vec3::new(0.98, 0.6, 0.38);
            let base = night_haze.lerp(day_haze, sky.state.light);
            let horizon_band = (1.0 - sky.state.sun_altitude.abs()).powi(2);
            let color = base.lerp(warm, horizon_band * (1.0 - sky.state.solar_occlusion) * 0.7);
            material.params.color = color.extend(1.0);
            material.params.horizon = horizon_uv;
            material.params.intensity = 0.25 + 0.6 * sky.state.light;
        }
    } else {
        if sky.state != CelestialState::default() {
            *sky = Sky::default();
        }
        if clear.0 != DEFAULT_CLEAR {
            clear.0 = DEFAULT_CLEAR;
        }
        for (_, mut visibility) in &mut quads.p0() {
            visibility.set_if_neq(Visibility::Hidden);
        }
        for (_, mut visibility) in &mut quads.p1() {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }

    let stars_on = synced && sky.state.star_visibility > 0.001;
    for (mut transform, mut visibility) in &mut quads.p2() {
        transform.scale = Vec3::new(native.x, native.y, 1.0);
        visibility.set_if_neq(if stars_on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (mut transform, mut visibility) in &mut quads.p3() {
        transform.scale = Vec3::new(native.x, native.y, 1.0);
        visibility.set_if_neq(if synced {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}
