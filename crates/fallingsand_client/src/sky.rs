use crate::camera::{CameraControl, VIRTUAL_HEIGHT, VIRTUAL_WIDTH};
use crate::net::{NetSet, ServerMsg, Session};
use crate::player::{PlayerVisual, PlayerVisuals};
use crate::worldview::WorldView;
use crate::{AppState, ClientRegistry, GameState};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use fallingsand_core::{Calendar, CellPos};
use fallingsand_protocol::ServerMessage;
use std::f32::consts::TAU;

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
const ORBIT_RADIUS_FRAC: f32 = 0.42;
const HORIZON_FRAC: f32 = 0.43;
const HORIZON_UV: f32 = 0.5 + HORIZON_FRAC * ORBIT_RADIUS_FRAC;

const MOON_LIGHT_MAX: f32 = 0.5;
const SKYGLOW: f32 = 0.03;

const INCLINATION_MAX: f32 = 0.576;
const SUN_DISC: f32 = 0.090;
const MOON_DISC: f32 = 0.096;
const UMBRA_R: f32 = 0.153;
const ECLIPSE_WIN: f32 = 0.05;
const SUN_DISC_FRAC: f32 = 0.35;
const MOON_DISC_FRAC: f32 = 0.90;

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
pub struct CelestialState {
    pub sun_alt: f32,
    pub solar_occ: f32,
    pub lunar_shadow: f32,
    pub light: f32,
    pub star_alpha: f32,
    pub sidereal: f32,
    pub synced: bool,
}

impl CelestialState {
    pub fn darkness(&self) -> f32 {
        (1.0 - self.light) * MAX_DARKNESS
    }
}

#[derive(ShaderType, Debug, Clone)]
pub struct DarknessParams {
    pub lights: [Vec4; MAX_LIGHTS],
    pub darkness: f32,
    pub light_count: u32,
}

impl Default for DarknessParams {
    fn default() -> Self {
        Self {
            lights: [Vec4::ZERO; MAX_LIGHTS],
            darkness: 0.0,
            light_count: 0,
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct DarknessMaterial {
    #[uniform(0)]
    pub params: DarknessParams,
}

impl Material2d for DarknessMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://fallingsand/shaders/darkness.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct SunParams {
    pub redness: f32,
    pub occlusion: f32,
    pub time: f32,
    pub _pad: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct SunMaterial {
    #[uniform(0)]
    pub params: SunParams,
}

impl Material2d for SunMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://fallingsand/shaders/sun.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct MoonParams {
    pub sun_dir: Vec2,
    pub illumination: f32,
    pub umbra: Vec2,
    pub umbra_r: f32,
    pub time: f32,
    pub sky_color: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct MoonMaterial {
    #[uniform(0)]
    pub params: MoonParams,
}

impl Material2d for MoonMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://fallingsand/shaders/moon.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct StarfieldParams {
    pub sidereal: f32,
    pub aspect: f32,
    pub star_alpha: f32,
    pub time: f32,
    pub horizon: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct StarfieldMaterial {
    #[uniform(0)]
    pub params: StarfieldParams,
}

impl Material2d for StarfieldMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://fallingsand/shaders/starfield.wgsl".into()
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
        "embedded://fallingsand/shaders/horizon.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Resource)]
struct SkyAssets {
    darkness: Handle<DarknessMaterial>,
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
struct DarknessQuad;

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
        bevy::asset::embedded_asset!(app, "shaders/darkness.wgsl");
        bevy::asset::embedded_asset!(app, "shaders/sun.wgsl");
        bevy::asset::embedded_asset!(app, "shaders/moon.wgsl");
        bevy::asset::embedded_asset!(app, "shaders/starfield.wgsl");
        bevy::asset::embedded_asset!(app, "shaders/horizon.wgsl");
        app.add_plugins(Material2dPlugin::<DarknessMaterial>::default())
            .add_plugins(Material2dPlugin::<SunMaterial>::default())
            .add_plugins(Material2dPlugin::<MoonMaterial>::default())
            .add_plugins(Material2dPlugin::<StarfieldMaterial>::default())
            .add_plugins(Material2dPlugin::<HorizonMaterial>::default())
            .init_resource::<WorldTime>()
            .init_resource::<CelestialState>()
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
    mut darkness_mats: ResMut<Assets<DarknessMaterial>>,
    mut sun_mats: ResMut<Assets<SunMaterial>>,
    mut moon_mats: ResMut<Assets<MoonMaterial>>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    mut horizon_mats: ResMut<Assets<HorizonMaterial>>,
    camera: Single<Entity, With<Camera2d>>,
) {
    let quad = meshes.add(Rectangle::default());
    let darkness = darkness_mats.add(DarknessMaterial {
        params: DarknessParams::default(),
    });
    let sun = sun_mats.add(SunMaterial::default());
    let moon = moon_mats.add(MoonMaterial::default());
    let starfield = star_mats.add(StarfieldMaterial::default());
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
            DarknessQuad,
            Mesh2d(quad),
            MeshMaterial2d(darkness.clone()),
            Transform::from_xyz(0.0, 0.0, 90.0),
            Visibility::Hidden,
        ));
    });
    commands.insert_resource(SkyAssets {
        darkness,
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
    real: Res<Time>,
    window: Single<&Window>,
    control: Res<CameraControl>,
    assets: Res<SkyAssets>,
    mut celestial: ResMut<CelestialState>,
    mut sun_mats: ResMut<Assets<SunMaterial>>,
    mut moon_mats: ResMut<Assets<MoonMaterial>>,
    mut sun_q: Query<(&mut Transform, &mut Visibility), (With<SunVisual>, Without<MoonVisual>)>,
    mut moon_q: Query<(&mut Transform, &mut Visibility), (With<MoonVisual>, Without<SunVisual>)>,
) {
    if !time.synced {
        return;
    }
    let cal = time.calendar;
    let r = view_size(&window, control.zoom).y.max(100.0) * ORBIT_RADIUS_FRAC;
    let center = Vec2::new(0.0, -HORIZON_FRAC * r);
    let t = real.elapsed_secs();

    let sun_ang = (cal.day_fraction() - 0.25) * TAU;
    let (sa, ca) = sun_ang.sin_cos();
    let sun_pos = Vec2::new(ca * r * 1.4, sa * r);

    let moon_ang = sun_ang - cal.elongation();
    let (sm, cm) = moon_ang.sin_cos();
    let beta = INCLINATION_MAX * cal.ecliptic_latitude();
    let radial = Vec2::new(cm, sm);
    let moon_pos = Vec2::new(cm * r * 1.4, sm * r) + radial * (beta * r);

    let syn = cal.synodic_fraction();
    let newness = (1.0 - syn.min(1.0 - syn) / ECLIPSE_WIN).clamp(0.0, 1.0);
    let fullness = (1.0 - (syn - 0.5).abs() / ECLIPSE_WIN).clamp(0.0, 1.0);
    let sep = (moon_pos - sun_pos).length() / r;
    let overlap = (1.0 - sep / (SUN_DISC + MOON_DISC)).clamp(0.0, 1.0);
    let solar_occ = overlap * newness;
    let shadow_sep = (moon_pos + sun_pos).length() / r;
    let lunar_shadow = (1.0 - shadow_sep / (UMBRA_R + MOON_DISC)).clamp(0.0, 1.0) * fullness;

    let moon_to_p = MOON_DISC_FRAC / (MOON_DISC * r);
    let umbra = (-sun_pos - moon_pos) * moon_to_p;
    let umbra_r = UMBRA_R * MOON_DISC_FRAC / MOON_DISC;

    let day_raw = smoothstep(-0.12, 0.10, sa);
    let sunlight = day_raw * (1.0 - solar_occ);
    let moon_up = smoothstep(-0.10, 0.10, (moon_pos.y / r).clamp(-1.0, 1.0));
    let moonlight = cal.moon_illumination() * moon_up * (1.0 - lunar_shadow) * MOON_LIGHT_MAX;
    let light = sunlight.max(moonlight + SKYGLOW).clamp(0.0, 1.0);
    let star_alpha = 1.0 - smoothstep(0.06, 0.25, light);
    let sun_dir = (sun_pos - moon_pos).normalize_or_zero();

    *celestial = CelestialState {
        sun_alt: sa,
        solar_occ,
        lunar_shadow,
        light,
        star_alpha,
        sidereal: cal.day_fraction(),
        synced: true,
    };

    if let Ok((mut tf, mut vis)) = sun_q.single_mut() {
        let s = 2.0 * SUN_DISC * r / SUN_DISC_FRAC;
        tf.translation = (sun_pos + center).extend(-50.0);
        tf.scale = Vec3::new(s, s, 1.0);
        *vis = Visibility::Inherited;
    }
    if let Some(mut material) = sun_mats.get_mut(&assets.sun) {
        material.params.redness = 1.0 - smoothstep(0.0, 0.35, sa);
        material.params.occlusion = solar_occ;
        material.params.time = t;
    }

    if let Ok((mut tf, mut vis)) = moon_q.single_mut() {
        let s = 2.0 * MOON_DISC * r / MOON_DISC_FRAC;
        tf.translation = (moon_pos + center).extend(-49.0);
        tf.scale = Vec3::new(s, s, 1.0);
        *vis = Visibility::Inherited;
    }
    if let Some(mut material) = moon_mats.get_mut(&assets.moon) {
        material.params.sun_dir = sun_dir;
        material.params.illumination = cal.moon_illumination();
        material.params.umbra = umbra;
        material.params.umbra_r = umbra_r;
        material.params.time = t;
        material.params.sky_color = sky_color(light, sa, solar_occ).extend(1.0);
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

fn update_sky_tint(cel: Res<CelestialState>, mut clear: ResMut<ClearColor>) {
    if !cel.synced {
        return;
    }
    let rgb = sky_color(cel.light, cel.sun_alt, cel.solar_occ);
    clear.0 = Color::srgb(rgb.x, rgb.y, rgb.z);
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn fit_fullscreen_quads(
    cel: Res<CelestialState>,
    real: Res<Time>,
    window: Single<&Window>,
    control: Res<CameraControl>,
    assets: Res<SkyAssets>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    mut horizon_mats: ResMut<Assets<HorizonMaterial>>,
    mut dark_q: Query<
        (&mut Transform, &mut Visibility),
        (With<DarknessQuad>, Without<StarfieldQuad>, Without<HorizonQuad>),
    >,
    mut star_q: Query<
        (&mut Transform, &mut Visibility),
        (With<StarfieldQuad>, Without<DarknessQuad>, Without<HorizonQuad>),
    >,
    mut horizon_q: Query<
        (&mut Transform, &mut Visibility),
        (With<HorizonQuad>, Without<DarknessQuad>, Without<StarfieldQuad>),
    >,
) {
    let size = view_size(&window, control.zoom) * 1.1;
    let dark_on = cel.synced && cel.darkness() > 0.001;
    for (mut tf, mut vis) in &mut dark_q {
        tf.scale = Vec3::new(size.x, size.y, 1.0);
        *vis = if dark_on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let star_on = cel.synced && cel.star_alpha > 0.001;
    for (mut tf, mut vis) in &mut star_q {
        tf.scale = Vec3::new(size.x, size.y, 1.0);
        *vis = if star_on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (mut tf, mut vis) in &mut horizon_q {
        tf.scale = Vec3::new(size.x, size.y, 1.0);
        *vis = if cel.synced {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Some(mut material) = star_mats.get_mut(&assets.starfield) {
        material.params.sidereal = cel.sidereal;
        material.params.aspect = (window.width() / window.height().max(1.0)).max(0.1);
        material.params.star_alpha = cel.star_alpha;
        material.params.time = real.elapsed_secs();
        material.params.horizon = HORIZON_UV;
    }
    if let Some(mut material) = horizon_mats.get_mut(&assets.horizon) {
        let day_haze = Vec3::new(0.72, 0.82, 0.96);
        let night_haze = Vec3::new(0.08, 0.11, 0.20);
        let warm = Vec3::new(0.98, 0.6, 0.38);
        let base = night_haze.lerp(day_haze, cel.light);
        let band = (1.0 - cel.sun_alt.abs()).powi(2);
        let col = base.lerp(warm, band * (1.0 - cel.solar_occ) * 0.7);
        material.params.color = col.extend(1.0);
        material.params.horizon = HORIZON_UV;
        material.params.intensity = 0.25 + 0.6 * cel.light;
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn scan_emissive(
    cel: Res<CelestialState>,
    real: Res<Time>,
    registry: Res<ClientRegistry>,
    view: Res<WorldView>,
    window: Single<&Window>,
    control: Res<CameraControl>,
    camera: Single<&Transform, With<Camera2d>>,
    mut emissive_lights: ResMut<EmissiveLights>,
    mut cooldown: Local<f32>,
) {
    *cooldown -= real.delta_secs();
    if *cooldown > 0.0 {
        return;
    }
    *cooldown = LIGHT_SCAN_INTERVAL;
    if !cel.synced || cel.darkness() <= 0.001 {
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
    cel: Res<CelestialState>,
    assets: Res<SkyAssets>,
    mut materials: ResMut<Assets<DarknessMaterial>>,
    emissive_lights: Res<EmissiveLights>,
    session: Option<Res<Session>>,
    visuals: Res<PlayerVisuals>,
    players: Query<(&Transform, &PlayerVisual)>,
) {
    let Some(mut material) = materials.get_mut(&assets.darkness) else {
        return;
    };
    let darkness = if cel.synced { cel.darkness() } else { 0.0 };
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
    mut celestial: ResMut<CelestialState>,
    mut clear: ResMut<ClearColor>,
    mut emissive_lights: ResMut<EmissiveLights>,
    assets: Option<Res<SkyAssets>>,
    mut darkness_mats: ResMut<Assets<DarknessMaterial>>,
    mut sun_mats: ResMut<Assets<SunMaterial>>,
    mut moon_mats: ResMut<Assets<MoonMaterial>>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    mut horizon_mats: ResMut<Assets<HorizonMaterial>>,
) {
    *time = WorldTime::default();
    *celestial = CelestialState::default();
    clear.0 = Color::srgb(0.08, 0.09, 0.13);
    emissive_lights.0.clear();
    if let Some(assets) = assets {
        if let Some(mut material) = darkness_mats.get_mut(&assets.darkness) {
            material.params = DarknessParams::default();
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
