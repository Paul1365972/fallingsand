use crate::camera::{
    CameraControl, SkyCamera, VIRTUAL_HEIGHT, VIRTUAL_WIDTH, WorldCamera, WorldTarget,
};
use crate::net::{NetSet, ServerMsg, Session};
use crate::player::{PlayerVisual, PlayerVisuals};
use crate::worldview::WorldView;
use crate::{AppState, ClientRegistry, GameState};
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use fallingsand_core::celestial::{MOON_DISC, UMBRA_RADIUS};
use fallingsand_core::{Calendar, CelestialState, CellPos};
use fallingsand_protocol::ServerMessage;

pub struct SkyPlugin;

const MAX_LIGHTS: usize = 32;
const MAX_DARKNESS: f32 = 0.82;
const PLAYER_LIGHT_RADIUS: f32 = 70.0;
const BURNING_LIGHT_RADIUS: f32 = 40.0;
const EMISSIVE_LIGHT_RADIUS: f32 = 28.0;
const EMISSIVE_MERGE_DIST: f32 = 24.0;
const EMISSIVE_MAX_RADIUS: f32 = 60.0;
const EMISSIVE_SCAN_STRIDE: i32 = 8;
const LIGHT_SCAN_INTERVAL: f32 = 0.1;
const HORIZON_FRAC: f32 = 0.43;
const ORBIT_RADIUS: f32 = 133.0;

const MOON_DISC_FRAC: f32 = 0.90;
const SUN_SIZE: f32 = 48.0;
const MOON_SIZE: f32 = 28.0;
const STAR_TEX_SIZE: f32 = 512.0;

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Resource, Default, Clone, Copy)]
pub struct WorldTime {
    pub calendar: Calendar,
    pub synced: bool,
}

impl WorldTime {
    pub fn moon_phase(&self) -> u32 {
        self.calendar.moon_phase()
    }
}

#[derive(Resource, Default, Clone, Copy)]
pub struct Sky {
    pub state: CelestialState,
    pub star_alpha: f32,
    pub synced: bool,
}

impl Sky {
    pub fn darkness(&self) -> f32 {
        (1.0 - self.state.light) * MAX_DARKNESS
    }
}

#[derive(ShaderType, Debug, Clone)]
pub struct LightingParams {
    pub lights: [Vec4; MAX_LIGHTS],
    pub darkness: f32,
    pub light_count: u32,
}

impl Default for LightingParams {
    fn default() -> Self {
        Self {
            lights: [Vec4::ZERO; MAX_LIGHTS],
            darkness: 0.0,
            light_count: 0,
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct LightingMaterial {
    #[uniform(0)]
    pub params: LightingParams,
    #[texture(1)]
    #[sampler(2)]
    pub world: Handle<Image>,
}

impl Material2d for LightingMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/lighting.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct SunParams {
    pub redness: f32,
    pub occlusion: f32,
    pub _pad: Vec2,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct SunMaterial {
    #[uniform(0)]
    pub params: SunParams,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Handle<Image>,
}

impl Material2d for SunMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/sun.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct MoonParams {
    pub sun_direction: Vec2,
    pub illumination: f32,
    pub umbra: Vec2,
    pub umbra_radius: f32,
    pub sky_color: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct MoonMaterial {
    #[uniform(0)]
    pub params: MoonParams,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Handle<Image>,
}

impl Material2d for MoonMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/moon.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct StarfieldParams {
    pub tiling: f32,
    pub aspect: f32,
    pub star_alpha: f32,
    pub horizon: f32,
    pub time: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct StarfieldMaterial {
    #[uniform(0)]
    pub params: StarfieldParams,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Handle<Image>,
}

impl Material2d for StarfieldMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/starfield.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct HorizonParams {
    pub color: Vec4,
    pub horizon: f32,
    pub intensity: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct HorizonMaterial {
    #[uniform(0)]
    pub params: HorizonParams,
}

impl Material2d for HorizonMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/horizon.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Resource)]
struct SkyAssets {
    lighting: Handle<LightingMaterial>,
    sun: Handle<SunMaterial>,
    moon: Handle<MoonMaterial>,
    starfield: Handle<StarfieldMaterial>,
    horizon: Handle<HorizonMaterial>,
}

#[derive(Resource, Default)]
struct EmissiveLights(Vec<Vec4>);

fn view_size(window: &Window, zoom: f32) -> Vec2 {
    let width = window.width().max(1.0);
    let height = window.height().max(1.0);
    let per_pixel = (VIRTUAL_WIDTH / width).max(VIRTUAL_HEIGHT / height) * zoom;
    Vec2::new(width, height) * per_pixel
}

#[derive(Component)]
struct LitWorldQuad;

#[derive(Component)]
struct StarfieldQuad;

#[derive(Component)]
struct HorizonQuad;

#[derive(Component)]
struct SunVisual;

#[derive(Component)]
struct MoonVisual;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<LightingMaterial>::default())
            .add_plugins(Material2dPlugin::<SunMaterial>::default())
            .add_plugins(Material2dPlugin::<MoonMaterial>::default())
            .add_plugins(Material2dPlugin::<StarfieldMaterial>::default())
            .add_plugins(Material2dPlugin::<HorizonMaterial>::default())
            .init_resource::<WorldTime>()
            .init_resource::<Sky>()
            .init_resource::<EmissiveLights>()
            .add_systems(PostStartup, setup_sky)
            .add_systems(
                PreUpdate,
                sync_time.after(NetSet).run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    update_orbits,
                    update_sky_tint,
                    fit_fullscreen_quads,
                    scan_emissive,
                    apply_lighting,
                )
                    .chain()
                    .after(crate::interpolation::interpolate)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(OnExit(AppState::InGame), reset_sky);
    }
}

#[allow(clippy::too_many_arguments)]
fn setup_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut lighting_mats: ResMut<Assets<LightingMaterial>>,
    mut sun_mats: ResMut<Assets<SunMaterial>>,
    mut moon_mats: ResMut<Assets<MoonMaterial>>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    mut horizon_mats: ResMut<Assets<HorizonMaterial>>,
    asset_server: Res<AssetServer>,
    world_target: Res<WorldTarget>,
    camera: Single<Entity, With<SkyCamera>>,
) {
    let quad = meshes.add(Rectangle::default());
    let lighting = lighting_mats.add(LightingMaterial {
        params: LightingParams::default(),
        world: world_target.0.clone(),
    });
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

    commands.entity(*camera).with_children(|parent| {
        parent.spawn((
            StarfieldQuad,
            Mesh2d(quad.clone()),
            MeshMaterial2d(starfield.clone()),
            Transform::from_xyz(0.0, 0.0, -60.0),
            Visibility::Hidden,
        ));
        parent.spawn((
            HorizonQuad,
            Mesh2d(quad.clone()),
            MeshMaterial2d(horizon.clone()),
            Transform::from_xyz(0.0, 0.0, -45.0),
            Visibility::Hidden,
        ));
        parent.spawn((
            SunVisual,
            Mesh2d(quad.clone()),
            MeshMaterial2d(sun.clone()),
            Transform::from_xyz(0.0, -1000.0, -50.0),
        ));
        parent.spawn((
            MoonVisual,
            Mesh2d(quad.clone()),
            MeshMaterial2d(moon.clone()),
            Transform::from_xyz(0.0, -1000.0, -49.0),
        ));
        parent.spawn((
            LitWorldQuad,
            Mesh2d(quad),
            MeshMaterial2d(lighting.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Visibility::Hidden,
        ));
    });
    commands.insert_resource(SkyAssets {
        lighting,
        sun,
        moon,
        starfield,
        horizon,
    });
}

fn sync_time(mut time: ResMut<WorldTime>, mut messages: MessageReader<ServerMsg>) {
    for ServerMsg(message) in messages.read() {
        if let ServerMessage::TickEnd { age, .. } = message {
            time.calendar.age = *age;
            time.synced = true;
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn update_orbits(
    time: Res<WorldTime>,
    assets: Res<SkyAssets>,
    mut sky: ResMut<Sky>,
    mut sun_mats: ResMut<Assets<SunMaterial>>,
    mut moon_mats: ResMut<Assets<MoonMaterial>>,
    mut sun_q: Query<(&mut Transform, &mut Visibility), (With<SunVisual>, Without<MoonVisual>)>,
    mut moon_q: Query<(&mut Transform, &mut Visibility), (With<MoonVisual>, Without<SunVisual>)>,
) {
    if !time.synced {
        return;
    }
    let calendar = time.calendar;
    let celestial = calendar.celestial();
    let radius = ORBIT_RADIUS;
    let center = Vec2::new(0.0, -HORIZON_FRAC * radius);

    let sun_position = Vec2::from(celestial.sun_position) * radius;
    let moon_position = Vec2::from(celestial.moon_position) * radius;
    let sun_altitude = celestial.sun_altitude;
    let solar_occlusion = celestial.solar_occlusion;

    let world_to_moon_uv = 2.0 / MOON_SIZE;
    let umbra = (-sun_position - moon_position) * world_to_moon_uv;
    let umbra_radius = UMBRA_RADIUS * MOON_DISC_FRAC / MOON_DISC;

    sky.state = celestial;
    sky.star_alpha = 1.0 - smoothstep(0.02, 0.45, celestial.light);
    sky.synced = true;

    if let Ok((mut transform, mut visibility)) = sun_q.single_mut() {
        transform.translation = (sun_position + center).extend(-50.0);
        transform.scale = Vec3::new(SUN_SIZE, SUN_SIZE, 1.0);
        *visibility = Visibility::Inherited;
    }
    if let Some(mut material) = sun_mats.get_mut(&assets.sun) {
        material.params.redness = 1.0 - smoothstep(0.0, 0.35, sun_altitude);
        material.params.occlusion = solar_occlusion;
    }

    if let Ok((mut transform, mut visibility)) = moon_q.single_mut() {
        transform.translation = (moon_position + center).extend(-49.0);
        transform.scale = Vec3::new(MOON_SIZE, MOON_SIZE, 1.0);
        *visibility = Visibility::Inherited;
    }
    if let Some(mut material) = moon_mats.get_mut(&assets.moon) {
        let color = sky_color(celestial.light, sun_altitude, solar_occlusion);
        let linear = Color::srgb(color.x, color.y, color.z).to_linear();
        material.params.sun_direction = Vec2::from(celestial.sun_direction);
        material.params.illumination = celestial.illumination;
        material.params.umbra = umbra;
        material.params.umbra_radius = umbra_radius;
        material.params.sky_color =
            Vec4::new(linear.red, linear.green, linear.blue, celestial.daylight);
    }
}

fn sky_color(light: f32, sun_alt: f32, solar_occ: f32) -> Vec3 {
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

fn update_sky_tint(sky: Res<Sky>, mut clear: ResMut<ClearColor>) {
    if !sky.synced {
        return;
    }
    let color = sky_color(
        sky.state.light,
        sky.state.sun_altitude,
        sky.state.solar_occlusion,
    );
    clear.0 = Color::srgb(color.x, color.y, color.z);
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn fit_fullscreen_quads(
    sky: Res<Sky>,
    real: Res<Time>,
    window: Single<&Window>,
    control: Res<CameraControl>,
    assets: Res<SkyAssets>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    mut horizon_mats: ResMut<Assets<HorizonMaterial>>,
    mut world_q: Query<
        (&mut Transform, &mut Visibility),
        (
            With<LitWorldQuad>,
            Without<StarfieldQuad>,
            Without<HorizonQuad>,
        ),
    >,
    mut star_q: Query<
        (&mut Transform, &mut Visibility),
        (
            With<StarfieldQuad>,
            Without<LitWorldQuad>,
            Without<HorizonQuad>,
        ),
    >,
    mut horizon_q: Query<
        (&mut Transform, &mut Visibility),
        (
            With<HorizonQuad>,
            Without<LitWorldQuad>,
            Without<StarfieldQuad>,
        ),
    >,
) {
    let view = view_size(&window, control.zoom);
    let size = view * 1.1;
    let horizon_uv = 0.5 + HORIZON_FRAC * ORBIT_RADIUS / view.y;
    for (mut transform, mut visibility) in &mut world_q {
        transform.scale = Vec3::new(view.x, view.y, 1.0);
        *visibility = if sky.synced {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let stars_on = sky.synced && sky.star_alpha > 0.001;
    for (mut transform, mut visibility) in &mut star_q {
        transform.scale = Vec3::new(size.x, size.y, 1.0);
        *visibility = if stars_on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (mut transform, mut visibility) in &mut horizon_q {
        transform.scale = Vec3::new(size.x, size.y, 1.0);
        *visibility = if sky.synced {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Some(mut material) = star_mats.get_mut(&assets.starfield) {
        material.params.tiling = (view_size(&window, 1.0).x * 1.1 / STAR_TEX_SIZE).max(0.05);
        material.params.aspect = (window.width() / window.height().max(1.0)).max(0.1);
        material.params.star_alpha = sky.star_alpha;
        material.params.horizon = horizon_uv;
        material.params.time = real.elapsed_secs();
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
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn scan_emissive(
    sky: Res<Sky>,
    real: Res<Time>,
    registry: Res<ClientRegistry>,
    view: Res<WorldView>,
    window: Single<&Window>,
    control: Res<CameraControl>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut emissive_lights: ResMut<EmissiveLights>,
    mut cooldown: Local<f32>,
) {
    *cooldown -= real.delta_secs();
    if *cooldown > 0.0 {
        return;
    }
    *cooldown = LIGHT_SCAN_INTERVAL;
    if !sky.synced || sky.darkness() <= 0.001 {
        if !emissive_lights.0.is_empty() {
            emissive_lights.0.clear();
        }
        return;
    }

    let mut lights: Vec<Vec4> = Vec::new();
    let center = camera.translation.truncate();
    let half = view_size(&window, control.zoom) / 2.0 + 32.0;
    let emissive = registry.0.tag_mask("emissive");
    let min_x =
        ((center.x - half.x) as i32).div_euclid(EMISSIVE_SCAN_STRIDE) * EMISSIVE_SCAN_STRIDE;
    let min_y =
        ((center.y - half.y) as i32).div_euclid(EMISSIVE_SCAN_STRIDE) * EMISSIVE_SCAN_STRIDE;
    let mut y = min_y;
    while y as f32 <= center.y + half.y {
        let mut x = min_x;
        while x as f32 <= center.x + half.x {
            let pos = CellPos::new(x, y);
            if let Some(cell) = view.get_cell(pos)
                && registry.0.has_tag(cell.material, emissive)
            {
                let point = Vec2::new(x as f32, y as f32);
                let mut merged = false;
                for light in lights.iter_mut() {
                    let existing = Vec2::new(light.x, light.y);
                    if existing.distance(point) < EMISSIVE_MERGE_DIST + light.z * 0.5 {
                        let mid = (existing + point) / 2.0;
                        let radius =
                            (light.z + existing.distance(point) * 0.5).min(EMISSIVE_MAX_RADIUS);
                        *light = Vec4::new(mid.x, mid.y, radius, light.w);
                        merged = true;
                        break;
                    }
                }
                if !merged && lights.len() < MAX_LIGHTS - 1 {
                    lights.push(Vec4::new(point.x, point.y, EMISSIVE_LIGHT_RADIUS, 0.9));
                }
            }
            x += EMISSIVE_SCAN_STRIDE;
        }
        y += EMISSIVE_SCAN_STRIDE;
    }
    emissive_lights.0 = lights;
}

fn apply_lighting(
    sky: Res<Sky>,
    assets: Res<SkyAssets>,
    mut materials: ResMut<Assets<LightingMaterial>>,
    emissive_lights: Res<EmissiveLights>,
    session: Option<Res<Session>>,
    visuals: Res<PlayerVisuals>,
    players: Query<(&Transform, &PlayerVisual)>,
) {
    let Some(mut material) = materials.get_mut(&assets.lighting) else {
        return;
    };
    let darkness = if sky.synced { sky.darkness() } else { 0.0 };
    material.params.darkness = darkness;
    if darkness <= 0.001 {
        material.params.light_count = 0;
        return;
    }

    let mut lights: Vec<Vec4> = Vec::new();
    let local = session.and_then(|session| session.player);
    if let Some(id) = local
        && let Some(&entity) = visuals.0.get(&id)
        && let Ok((transform, _)) = players.get(entity)
    {
        lights.push(Vec4::new(
            transform.translation.x,
            transform.translation.y,
            PLAYER_LIGHT_RADIUS,
            1.0,
        ));
    }
    for (transform, visual) in &players {
        if visual.burning && lights.len() < MAX_LIGHTS {
            lights.push(Vec4::new(
                transform.translation.x,
                transform.translation.y,
                BURNING_LIGHT_RADIUS,
                1.0,
            ));
        }
    }
    for light in &emissive_lights.0 {
        if lights.len() >= MAX_LIGHTS {
            break;
        }
        lights.push(*light);
    }

    material.params.light_count = lights.len().min(MAX_LIGHTS) as u32;
    let mut array = [Vec4::ZERO; MAX_LIGHTS];
    for (slot, light) in array.iter_mut().zip(lights.iter()) {
        *slot = *light;
    }
    material.params.lights = array;
}

#[allow(clippy::too_many_arguments)]
fn reset_sky(
    mut time: ResMut<WorldTime>,
    mut sky: ResMut<Sky>,
    mut clear: ResMut<ClearColor>,
    mut emissive_lights: ResMut<EmissiveLights>,
    assets: Option<Res<SkyAssets>>,
    mut lighting_mats: ResMut<Assets<LightingMaterial>>,
    mut sun_mats: ResMut<Assets<SunMaterial>>,
    mut moon_mats: ResMut<Assets<MoonMaterial>>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    mut horizon_mats: ResMut<Assets<HorizonMaterial>>,
) {
    *time = WorldTime::default();
    *sky = Sky::default();
    clear.0 = Color::srgb(0.08, 0.09, 0.13);
    emissive_lights.0.clear();
    if let Some(assets) = assets {
        if let Some(mut material) = lighting_mats.get_mut(&assets.lighting) {
            material.params = LightingParams::default();
        }
        if let Some(mut material) = sun_mats.get_mut(&assets.sun) {
            material.params = SunParams::default();
        }
        if let Some(mut material) = moon_mats.get_mut(&assets.moon) {
            material.params = MoonParams::default();
        }
        if let Some(mut material) = star_mats.get_mut(&assets.starfield) {
            material.params = StarfieldParams::default();
        }
        if let Some(mut material) = horizon_mats.get_mut(&assets.horizon) {
            material.params = HorizonParams::default();
        }
    }
}
