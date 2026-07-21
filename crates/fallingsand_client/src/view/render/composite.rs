use super::extract::PixelViewport;
use super::light_field;
use super::primitives::DebugLine;
use super::sky::{AtmosphereVisual, MoonVisual, Sky, SkyVisuals, StarfieldVisual, SunVisual};
use super::targets::RenderTargets;
use super::{color_attachment, pipeline, populated, queue_pipeline};
use crate::game::RenderMode;
use crate::view::Game;
use crate::view::camera::CameraState;
use bevy::core_pipeline::FullscreenShader;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only, texture_2d, uniform_buffer,
};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use bevy::render::texture::{FallbackImageZero, GpuImage};
use bevy::render::view::ViewTarget;

const FAR_RATIO: Vec2 = Vec2::new(0.88, 0.92);
const NEAR_RATIO: Vec2 = Vec2::new(0.72, 0.80);
const WALL_RATIO: Vec2 = Vec2::splat(0.15);
const WALL_COLOR: Vec3 = Vec3::new(0.060, 0.052, 0.048);
const FAR_HAZE: f32 = 0.6;
const NEAR_HAZE: f32 = 0.35;
const FAR_BASE: f32 = 14.0;
const FAR_AMP: f32 = 90.0;
const FAR_WAVELENGTH: f32 = 220.0;
const NEAR_BASE: f32 = 4.0;
const NEAR_AMP: f32 = 45.0;
const NEAR_WAVELENGTH: f32 = 90.0;
const PLAYER_LIGHT_RADIUS: f32 = 40.0;
const BURNING_LIGHT_RADIUS: f32 = 64.0;

#[derive(Clone, ShaderType, Default)]
struct SunFrame {
    redness: f32,
    occlusion: f32,
    quad_size: f32,
    disc_radius: f32,
}

impl From<&SunVisual> for SunFrame {
    fn from(value: &SunVisual) -> Self {
        Self {
            redness: value.redness,
            occlusion: value.occlusion,
            quad_size: value.quad_size,
            disc_radius: value.disc_radius,
        }
    }
}

#[derive(Clone, ShaderType, Default)]
struct MoonFrame {
    sun_direction: Vec2,
    illumination: f32,
    umbra: Vec2,
    umbra_radius: f32,
    sky_color: Vec4,
    quad_size: f32,
    disc_radius: f32,
    lunar_shadow: f32,
}

impl From<&MoonVisual> for MoonFrame {
    fn from(value: &MoonVisual) -> Self {
        Self {
            sun_direction: value.sun_direction,
            illumination: value.illumination,
            umbra: value.umbra,
            umbra_radius: value.umbra_radius,
            sky_color: value.sky_color,
            quad_size: value.quad_size,
            disc_radius: value.disc_radius,
            lunar_shadow: value.lunar_shadow,
        }
    }
}

#[derive(Clone, ShaderType, Default)]
struct StarfieldFrame {
    center: Vec2,
    scroll: Vec2,
    world_scale: f32,
    star_visibility: f32,
    horizon: f32,
    sidereal: f32,
}

impl From<&StarfieldVisual> for StarfieldFrame {
    fn from(value: &StarfieldVisual) -> Self {
        Self {
            center: value.center,
            scroll: value.scroll,
            world_scale: value.world_scale,
            star_visibility: value.star_visibility,
            horizon: value.horizon,
            sidereal: value.sidereal,
        }
    }
}

#[derive(Clone, ShaderType, Default)]
struct AtmosphereFrame {
    color: Vec4,
    sun_pos: Vec2,
    moon_pos: Vec2,
    sun_glow: Vec4,
    moon_glow: Vec4,
    horizon: f32,
    intensity: f32,
    aspect: f32,
    _pad: f32,
}

impl From<&AtmosphereVisual> for AtmosphereFrame {
    fn from(value: &AtmosphereVisual) -> Self {
        Self {
            color: value.color,
            sun_pos: value.sun_pos,
            moon_pos: value.moon_pos,
            sun_glow: value.sun_glow,
            moon_glow: value.moon_glow,
            horizon: value.horizon,
            intensity: value.intensity,
            aspect: value.aspect,
            _pad: 0.0,
        }
    }
}

#[derive(Clone, ShaderType, Default)]
struct CelestialFrame {
    sun: SunFrame,
    moon: MoonFrame,
    stars: StarfieldFrame,
    atmosphere: AtmosphereFrame,
    sun_center: Vec2,
    sun_size: Vec2,
    moon_center: Vec2,
    moon_size: Vec2,
}

impl From<&SkyVisuals> for CelestialFrame {
    fn from(value: &SkyVisuals) -> Self {
        Self {
            sun: (&value.sun).into(),
            moon: (&value.moon).into(),
            stars: (&value.stars).into(),
            atmosphere: (&value.atmosphere).into(),
            sun_center: value.sun_quad.center_px,
            sun_size: value.sun_quad.size_px,
            moon_center: value.moon_quad.center_px,
            moon_size: value.moon_quad.size_px,
        }
    }
}

#[derive(Clone, ShaderType, Default)]
struct LightingFrame {
    darkness: f32,
    light_count: u32,
    snapped_cam: Vec2,
    margin: Vec2,
}

#[derive(Clone, ShaderType, Default)]
struct WallFrame {
    base_color: Vec4,
    world_offset: Vec2,
}

#[derive(Clone, ShaderType, Default)]
struct SilhouetteFrame {
    color: Vec4,
    snapped_cam: Vec2,
    base: f32,
    amplitude: f32,
    inv_wavelength: f32,
    seed: f32,
}

#[derive(Clone, ShaderType, Default)]
struct WorldFrame {
    lighting: LightingFrame,
    wall: WallFrame,
    far: SilhouetteFrame,
    near: SilhouetteFrame,
    wall_snapped: Vec2,
}

#[derive(Clone, ShaderType, Default)]
struct BackdropFrame {
    celestial: CelestialFrame,
    star_offset: Vec2,
    far_offset: Vec2,
    near_offset: Vec2,
    wall_offset: Vec2,
}

#[derive(Clone, ShaderType)]
pub(super) struct SceneFrame {
    viewport: PixelViewport,
    world: WorldFrame,
    backdrop: BackdropFrame,
    world_offset: Vec2,
    clear_color: Vec4,
    backdrop_ready: u32,
}

impl Default for SceneFrame {
    fn default() -> Self {
        Self {
            viewport: default(),
            world: default(),
            backdrop: default(),
            world_offset: Vec2::ZERO,
            clear_color: Vec4::ZERO,
            backdrop_ready: 0,
        }
    }
}

#[derive(Clone, ShaderType)]
pub(super) struct PointLight {
    center: Vec2,
    radius: f32,
    intensity: f32,
}

#[derive(Clone, ShaderType)]
struct OverlayFrame {
    window_size: Vec2,
}

impl Default for OverlayFrame {
    fn default() -> Self {
        Self {
            window_size: Vec2::ONE,
        }
    }
}

#[derive(Clone, ShaderType)]
pub(super) struct LineInstance {
    a: Vec2,
    b: Vec2,
    color: Vec4,
}

impl From<DebugLine> for LineInstance {
    fn from(line: DebugLine) -> Self {
        Self {
            a: line.a,
            b: line.b,
            color: line.color,
        }
    }
}

fn layer_offset(state: &CameraState, ratio: Vec2, drift: Vec2) -> Vec2 {
    let (_, remainder) = state.layer(ratio);
    let raw = remainder + drift;
    match state.render_mode {
        RenderMode::PixelPerfect => -raw.round(),
        RenderMode::Smooth => -raw,
        RenderMode::Retro => Vec2::ZERO,
    }
}

pub(super) fn point_lights(game: &Game, sky: &Sky) -> Vec<PointLight> {
    let mut lights = Vec::new();
    if sky.darkness() <= 0.001 {
        return lights;
    }
    let Some(ingame) = game.0.ingame() else {
        return lights;
    };
    if let Some(local) = ingame.local_avatar() {
        lights.push(PointLight {
            center: local.pos,
            radius: if local.burning {
                BURNING_LIGHT_RADIUS
            } else {
                PLAYER_LIGHT_RADIUS
            },
            intensity: 1.0,
        });
    }
    let local = ingame
        .net
        .session
        .as_ref()
        .and_then(|session| session.player());
    for (&player, remote) in &ingame.players.avatars {
        if Some(player) != local && remote.burning {
            lights.push(PointLight {
                center: remote.pos,
                radius: BURNING_LIGHT_RADIUS,
                intensity: 1.0,
            });
        }
    }
    lights
}

pub(super) fn scene_frame(
    viewport: PixelViewport,
    state: &CameraState,
    sky: &Sky,
    clear_color: Vec4,
    light_count: usize,
) -> SceneFrame {
    let sky_linear = if sky.synced {
        sky.color_linear
    } else {
        Vec3::ZERO
    };
    let (world_snapped, _) = state.layer(Vec2::ZERO);
    let (wall_snapped, _) = state.layer(WALL_RATIO);
    let (far_snapped, _) = state.layer(FAR_RATIO);
    let (near_snapped, _) = state.layer(NEAR_RATIO);
    let star_drift = (sky.visuals.star_scroll - sky.visuals.star_scroll.floor()) * state.k as f32;
    SceneFrame {
        viewport,
        world: WorldFrame {
            lighting: LightingFrame {
                darkness: if sky.synced { sky.darkness() } else { 0.0 },
                light_count: light_count as u32,
                snapped_cam: world_snapped.as_vec2(),
                margin: light_field::margin(state.native),
            },
            wall: WallFrame {
                base_color: WALL_COLOR.extend(1.0),
                world_offset: WALL_RATIO * state.pos,
            },
            far: SilhouetteFrame {
                color: (sky_linear * FAR_HAZE).extend(1.0),
                snapped_cam: far_snapped.as_vec2(),
                base: FAR_BASE,
                amplitude: FAR_AMP,
                inv_wavelength: 1.0 / FAR_WAVELENGTH,
                seed: 17.0,
            },
            near: SilhouetteFrame {
                color: (sky_linear * NEAR_HAZE).extend(1.0),
                snapped_cam: near_snapped.as_vec2(),
                base: NEAR_BASE,
                amplitude: NEAR_AMP,
                inv_wavelength: 1.0 / NEAR_WAVELENGTH,
                seed: 53.0,
            },
            wall_snapped: wall_snapped.as_vec2(),
        },
        backdrop: BackdropFrame {
            celestial: (&sky.visuals).into(),
            star_offset: layer_offset(state, Vec2::ONE, star_drift),
            far_offset: layer_offset(state, FAR_RATIO, Vec2::ZERO),
            near_offset: layer_offset(state, NEAR_RATIO, Vec2::ZERO),
            wall_offset: layer_offset(state, WALL_RATIO, Vec2::ZERO),
        },
        world_offset: layer_offset(state, Vec2::ZERO, Vec2::ZERO),
        clear_color,
        backdrop_ready: u32::from(sky.synced),
    }
}

#[derive(Resource)]
pub(super) struct CompositePass {
    scene_layout: BindGroupLayoutDescriptor,
    overlay_layout: BindGroupLayoutDescriptor,
    scene_pipeline: CachedRenderPipelineId,
    overlay_pipeline: CachedRenderPipelineId,
    scene_frame: UniformBuffer<SceneFrame>,
    overlay_frame: UniformBuffer<OverlayFrame>,
    lights: StorageBuffer<Vec<PointLight>>,
    lines: StorageBuffer<Vec<LineInstance>>,
    scene_bind_group: Option<BindGroup>,
    overlay_bind_group: Option<BindGroup>,
    target_revision: u64,
    star_view: Option<TextureViewId>,
    fallback_star_view: TextureView,
    fallback_star_sampler: Sampler,
    linear_sampler: Sampler,
}

impl CompositePass {
    pub(super) fn new(
        device: &RenderDevice,
        asset_server: &AssetServer,
        fullscreen: &FullscreenShader,
        fallback: &FallbackImageZero,
        cache: &PipelineCache,
    ) -> Self {
        let scene_layout = BindGroupLayoutDescriptor::new(
            "game_composite_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    uniform_buffer::<SceneFrame>(false),
                    storage_buffer_read_only::<Vec<PointLight>>(false),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                ),
            ),
        );
        let overlay_layout = BindGroupLayoutDescriptor::new(
            "game_overlay_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    uniform_buffer::<OverlayFrame>(false),
                    storage_buffer_read_only::<Vec<LineInstance>>(false),
                ),
            ),
        );
        let composite_shader = asset_server.load("shaders/composite.wgsl");
        let overlay_shader = asset_server.load("shaders/overlay.wgsl");
        let scene_pipeline = queue_pipeline(
            cache,
            "game_composite_pipeline",
            vec![scene_layout.clone()],
            fullscreen.to_vertex_state(),
            composite_shader,
            "composite_fragment",
            None,
        );
        let overlay_pipeline = queue_pipeline(
            cache,
            "game_overlay_pipeline",
            vec![overlay_layout.clone()],
            VertexState {
                shader: overlay_shader.clone(),
                entry_point: Some("line_vertex".into()),
                ..default()
            },
            overlay_shader,
            "line_fragment",
            Some(BlendState::ALPHA_BLENDING),
        );
        let mut scene_frame = UniformBuffer::from(SceneFrame::default());
        scene_frame.set_label(Some("game_scene_frame"));
        let mut overlay_frame = UniformBuffer::from(OverlayFrame::default());
        overlay_frame.set_label(Some("game_overlay_frame"));
        let mut lights = StorageBuffer::from(vec![PointLight {
            center: Vec2::ZERO,
            radius: 0.0,
            intensity: 0.0,
        }]);
        lights.set_label(Some("game_point_lights"));
        let mut lines = StorageBuffer::from(vec![LineInstance {
            a: Vec2::ZERO,
            b: Vec2::ZERO,
            color: Vec4::ZERO,
        }]);
        lines.set_label(Some("game_debug_lines"));
        Self {
            scene_layout,
            overlay_layout,
            scene_pipeline,
            overlay_pipeline,
            scene_frame,
            overlay_frame,
            lights,
            lines,
            scene_bind_group: None,
            overlay_bind_group: None,
            target_revision: u64::MAX,
            star_view: None,
            fallback_star_view: fallback.texture_view.clone(),
            fallback_star_sampler: fallback.sampler.clone(),
            linear_sampler: device.create_sampler(&SamplerDescriptor {
                label: Some("game_linear_sampler"),
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                ..default()
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        scene: &SceneFrame,
        lights: &[PointLight],
        lines: &[LineInstance],
        star: &Handle<Image>,
        targets: &RenderTargets,
        device: &RenderDevice,
        queue: &RenderQueue,
        images: &RenderAssets<GpuImage>,
        cache: &PipelineCache,
    ) {
        self.scene_frame.set(scene.clone());
        self.scene_frame.write_buffer(device, queue);
        self.overlay_frame.set(OverlayFrame {
            window_size: scene.viewport.window_size,
        });
        self.overlay_frame.write_buffer(device, queue);
        let light_buffer = self.lights.buffer().map(Buffer::id);
        let line_buffer = self.lines.buffer().map(Buffer::id);
        self.lights.set(populated(
            lights,
            PointLight {
                center: Vec2::ZERO,
                radius: 0.0,
                intensity: 0.0,
            },
        ));
        self.lights.write_buffer(device, queue);
        self.lines.set(populated(
            lines,
            LineInstance {
                a: Vec2::ZERO,
                b: Vec2::ZERO,
                color: Vec4::ZERO,
            },
        ));
        self.lines.write_buffer(device, queue);
        let light_buffer_changed = light_buffer != self.lights.buffer().map(Buffer::id);
        let line_buffer_changed = line_buffer != self.lines.buffer().map(Buffer::id);
        let (star_view, star_sampler) = images.get(star.id()).map_or_else(
            || {
                (
                    self.fallback_star_view.clone(),
                    self.fallback_star_sampler.clone(),
                )
            },
            |image| (image.texture_view.clone(), image.sampler.clone()),
        );
        let star_view_id = star_view.id();
        if self.target_revision != targets.revision
            || self.star_view != Some(star_view_id)
            || light_buffer_changed
            || self.scene_bind_group.is_none()
        {
            let layout = cache.get_bind_group_layout(&self.scene_layout);
            self.scene_bind_group = Some(device.create_bind_group(
                "game_composite_bind_group",
                &layout,
                &BindGroupEntries::sequential((
                    self.scene_frame.binding().expect("scene frame written"),
                    self.lights.binding().expect("point lights written"),
                    &targets.world.view,
                    &targets.emission.view,
                    &targets.light.view,
                    &self.linear_sampler,
                    &star_view,
                    &star_sampler,
                )),
            ));
            self.target_revision = targets.revision;
            self.star_view = Some(star_view_id);
        }
        if line_buffer_changed || self.overlay_bind_group.is_none() {
            let layout = cache.get_bind_group_layout(&self.overlay_layout);
            self.overlay_bind_group = Some(device.create_bind_group(
                "game_overlay_bind_group",
                &layout,
                &BindGroupEntries::sequential((
                    self.overlay_frame.binding().expect("overlay frame written"),
                    self.lines.binding().expect("debug lines written"),
                )),
            ));
        }
    }

    pub(super) fn draw(
        &self,
        context: &mut RenderContext,
        view_target: &ViewTarget,
        line_count: u32,
        cache: &PipelineCache,
    ) {
        let Some(scene_pipeline) = pipeline(cache, self.scene_pipeline) else {
            return;
        };
        let Some(scene_bind_group) = self.scene_bind_group.as_ref() else {
            return;
        };
        let mut pass = context
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("game_composite_pass"),
                color_attachments: &[Some(color_attachment(
                    view_target.main_texture_view(),
                    Some(Color::NONE),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        pass.set_pipeline(scene_pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.draw(0..3, 0..1);
        if line_count == 0 {
            return;
        }
        let Some(overlay_pipeline) = pipeline(cache, self.overlay_pipeline) else {
            return;
        };
        let Some(overlay_bind_group) = self.overlay_bind_group.as_ref() else {
            return;
        };
        pass.set_pipeline(overlay_pipeline);
        pass.set_bind_group(0, overlay_bind_group, &[]);
        pass.draw(0..6, 0..line_count);
    }
}
