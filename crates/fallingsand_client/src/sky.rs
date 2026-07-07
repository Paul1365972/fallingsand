use crate::camera::{CameraControl, VIRTUAL_HEIGHT, VIRTUAL_WIDTH};
use crate::net::{NetSet, ServerMsg, Session};
use crate::player::{PlayerVisual, PlayerVisuals};
use crate::worldview::WorldView;
use crate::{AppState, ClientRegistry, GameState};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use fallingsand_core::{Calendar, CellPos, MOON_PHASES};
use fallingsand_protocol::ServerMessage;

pub struct SkyPlugin;

const MAX_LIGHTS: usize = 32;
const MAX_DARKNESS: f32 = 0.82;
const FULL_MOON_DARKNESS_MULT: f32 = 0.7;
const PLAYER_LIGHT_RADIUS: f32 = 70.0;
const BURNING_LIGHT_RADIUS: f32 = 40.0;
const EMISSIVE_LIGHT_RADIUS: f32 = 28.0;
const EMISSIVE_MERGE_DIST: f32 = 24.0;
const EMISSIVE_MAX_RADIUS: f32 = 60.0;
const EMISSIVE_SCAN_STRIDE: i32 = 8;
const LIGHT_SCAN_INTERVAL: f32 = 0.1;
const ORBIT_RADIUS_FRAC: f32 = 0.42;

const SKY_KEYFRAMES: &[(f32, [f32; 3])] = &[
    (0.0, [0.015, 0.025, 0.055]),
    (0.20, [0.03, 0.04, 0.09]),
    (0.27, [0.50, 0.32, 0.28]),
    (0.35, [0.33, 0.50, 0.76]),
    (0.50, [0.40, 0.60, 0.86]),
    (0.65, [0.33, 0.50, 0.76]),
    (0.73, [0.55, 0.30, 0.24]),
    (0.80, [0.03, 0.04, 0.09]),
    (1.0, [0.015, 0.025, 0.055]),
];

const DARKNESS_KEYFRAMES: &[(f32, f32)] = &[
    (0.0, 1.0),
    (0.22, 1.0),
    (0.32, 0.0),
    (0.68, 0.0),
    (0.78, 1.0),
    (1.0, 1.0),
];

#[derive(Resource, Default, Clone, Copy)]
pub struct WorldTime {
    pub calendar: Calendar,
    pub synced: bool,
}

impl WorldTime {
    pub fn day_fraction(&self) -> f32 {
        self.calendar.day_fraction()
    }

    pub fn moon_phase(&self) -> u32 {
        self.calendar.moon_phase()
    }

    pub fn moon_fullness(&self) -> f32 {
        let phase = self.moon_phase() as f32;
        1.0 - (phase - MOON_PHASES as f32 / 2.0).abs() / (MOON_PHASES as f32 / 2.0)
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

#[derive(Resource)]
struct SkyAssets {
    darkness: Handle<DarknessMaterial>,
    moon_phases: Vec<Handle<Image>>,
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
struct SunVisual;

#[derive(Component)]
struct MoonVisual;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "shaders/darkness.wgsl");
        app.add_plugins(Material2dPlugin::<DarknessMaterial>::default())
            .init_resource::<WorldTime>()
            .init_resource::<EmissiveLights>()
            .add_systems(PostStartup, setup_sky)
            .add_systems(
                PreUpdate,
                sync_time.after(NetSet).run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    update_sky_tint,
                    update_orbits,
                    scan_emissive,
                    apply_lighting,
                    fit_darkness_quad,
                )
                    .chain()
                    .after(crate::interpolation::interpolate)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(OnExit(AppState::InGame), reset_sky);
    }
}

fn circle_image(radius: u32, color: [u8; 4]) -> Image {
    let size = radius * 2 + 2;
    let mut data = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            if dx * dx + dy * dy > radius as f32 * radius as f32 {
                continue;
            }
            let index = ((y * size + x) * 4) as usize;
            data[index..index + 4].copy_from_slice(&color);
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

const MOON_LIT: [f32; 3] = [222.0, 228.0, 240.0];
const MOON_MARIA: [f32; 3] = [178.0, 186.0, 205.0];
const MOON_DARK: [f32; 3] = [46.0, 52.0, 70.0];
const MOON_DARK_ALPHA: f32 = 0.22;
const MOON_MARIA_SPOTS: &[(f32, f32, f32)] = &[
    (-0.38, -0.22, 0.36),
    (0.28, 0.12, 0.30),
    (-0.02, 0.48, 0.22),
    (0.44, -0.42, 0.18),
];

fn moon_image(radius: u32, phase: u32, phases: u32) -> Image {
    let size = radius * 2 + 2;
    let mut data = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 / 2.0;
    let r = radius as f32;
    let cycle = phase as f32 / phases as f32;
    let terminator = (cycle * std::f32::consts::TAU).cos();
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let disc = (r - dist + 0.5).clamp(0.0, 1.0);
            if disc <= 0.0 {
                continue;
            }
            let half_width = (r * r - dy * dy).max(0.0).sqrt().max(1e-3);
            let edge = terminator * half_width;
            let lit = if cycle < 0.5 {
                (dx - edge + 0.5).clamp(0.0, 1.0)
            } else {
                (-edge - dx + 0.5).clamp(0.0, 1.0)
            };
            let surface = if MOON_MARIA_SPOTS.iter().any(|&(mx, my, mr)| {
                let sx = dx / r - mx;
                let sy = dy / r - my;
                sx * sx + sy * sy < mr * mr
            }) {
                MOON_MARIA
            } else {
                MOON_LIT
            };
            let alpha = disc * (MOON_DARK_ALPHA + (1.0 - MOON_DARK_ALPHA) * lit);
            let index = ((y * size + x) * 4) as usize;
            for channel in 0..3 {
                let value = MOON_DARK[channel] + (surface[channel] - MOON_DARK[channel]) * lit;
                data[index + channel] = value.round() as u8;
            }
            data[index + 3] = (alpha * 255.0).round() as u8;
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn setup_sky(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<DarknessMaterial>>,
    camera: Single<Entity, With<Camera2d>>,
) {
    let sun = images.add(circle_image(9, [255, 232, 150, 255]));
    let moon_phases: Vec<Handle<Image>> = (0..MOON_PHASES)
        .map(|phase| images.add(moon_image(7, phase, MOON_PHASES)))
        .collect();

    let darkness = materials.add(DarknessMaterial {
        params: DarknessParams::default(),
    });
    let quad = meshes.add(Rectangle::default());

    commands.entity(*camera).with_children(|parent| {
        parent.spawn((
            SunVisual,
            Sprite::from_image(sun),
            Transform::from_xyz(0.0, -1000.0, -50.0),
        ));
        parent.spawn((
            MoonVisual,
            Sprite::from_image(moon_phases[0].clone()),
            Transform::from_xyz(0.0, -1000.0, -50.0),
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
        moon_phases,
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

fn sample_keyframes(keyframes: &[(f32, [f32; 3])], t: f32) -> [f32; 3] {
    let mut previous = keyframes[0];
    for &frame in keyframes {
        if frame.0 >= t {
            let span = (frame.0 - previous.0).max(1e-6);
            let mix = ((t - previous.0) / span).clamp(0.0, 1.0);
            return [
                previous.1[0] + (frame.1[0] - previous.1[0]) * mix,
                previous.1[1] + (frame.1[1] - previous.1[1]) * mix,
                previous.1[2] + (frame.1[2] - previous.1[2]) * mix,
            ];
        }
        previous = frame;
    }
    keyframes[keyframes.len() - 1].1
}

fn sample_scalar(keyframes: &[(f32, f32)], t: f32) -> f32 {
    let mut previous = keyframes[0];
    for &frame in keyframes {
        if frame.0 >= t {
            let span = (frame.0 - previous.0).max(1e-6);
            let mix = ((t - previous.0) / span).clamp(0.0, 1.0);
            return previous.1 + (frame.1 - previous.1) * mix;
        }
        previous = frame;
    }
    keyframes[keyframes.len() - 1].1
}

fn update_sky_tint(time: Res<WorldTime>, mut clear: ResMut<ClearColor>) {
    if !time.synced {
        return;
    }
    let rgb = sample_keyframes(SKY_KEYFRAMES, time.day_fraction());
    clear.0 = Color::srgb(rgb[0], rgb[1], rgb[2]);
}

fn night_darkness(time: &WorldTime) -> f32 {
    let base = sample_scalar(DARKNESS_KEYFRAMES, time.day_fraction());
    let phase_mult = 1.0 - (1.0 - FULL_MOON_DARKNESS_MULT) * time.moon_fullness();
    base * MAX_DARKNESS * phase_mult
}

#[allow(clippy::type_complexity)]
fn update_orbits(
    time: Res<WorldTime>,
    assets: Res<SkyAssets>,
    window: Single<&Window>,
    control: Res<CameraControl>,
    mut sun: Query<(&mut Transform, &mut Visibility), (With<SunVisual>, Without<MoonVisual>)>,
    mut moon: Query<
        (&mut Transform, &mut Visibility, &mut Sprite),
        (With<MoonVisual>, Without<SunVisual>),
    >,
) {
    if !time.synced {
        return;
    }
    let radius = view_size(&window, control.zoom).y.max(100.0) * ORBIT_RADIUS_FRAC;
    let angle = (time.day_fraction() - 0.25) * std::f32::consts::TAU;
    let sun_pos = Vec2::new(angle.cos() * radius * 1.4, angle.sin() * radius);
    if let Ok((mut transform, mut visibility)) = sun.single_mut() {
        transform.translation = sun_pos.extend(-50.0);
        *visibility = if sun_pos.y > -radius * 0.25 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let moon_pos = -sun_pos;
    if let Ok((mut transform, mut visibility, mut sprite)) = moon.single_mut() {
        transform.translation = moon_pos.extend(-50.0);
        *visibility = if moon_pos.y > -radius * 0.25 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let image = assets.moon_phases[time.moon_phase() as usize].clone();
        if sprite.image != image {
            sprite.image = image;
        }
    }
}

fn fit_darkness_quad(
    time: Res<WorldTime>,
    window: Single<&Window>,
    control: Res<CameraControl>,
    mut quad: Query<(&mut Transform, &mut Visibility), With<DarknessQuad>>,
) {
    let size = view_size(&window, control.zoom) * 1.1;
    let dark = time.synced && night_darkness(&time) > 0.001;
    for (mut transform, mut visibility) in &mut quad {
        transform.scale = Vec3::new(size.x, size.y, 1.0);
        *visibility = if dark {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn scan_emissive(
    time: Res<WorldTime>,
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
    if !time.synced || night_darkness(&time) <= 0.001 {
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
    time: Res<WorldTime>,
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
    let darkness = if time.synced {
        night_darkness(&time)
    } else {
        0.0
    };
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

fn reset_sky(
    mut time: ResMut<WorldTime>,
    mut clear: ResMut<ClearColor>,
    mut emissive_lights: ResMut<EmissiveLights>,
    assets: Option<Res<SkyAssets>>,
    mut materials: ResMut<Assets<DarknessMaterial>>,
) {
    *time = WorldTime::default();
    clear.0 = Color::srgb(0.08, 0.09, 0.13);
    emissive_lights.0.clear();
    if let Some(assets) = assets
        && let Some(mut material) = materials.get_mut(&assets.darkness)
    {
        material.params = DarknessParams::default();
    }
}
