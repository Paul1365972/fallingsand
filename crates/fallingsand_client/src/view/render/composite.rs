use super::extract::CompositeExtract;
use super::scene::{LineInstance, PointLight, SceneFrame};
use super::targets::RenderTargets;
use super::{color_attachment, pipeline, populated, queue_pipeline};
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

    pub(super) fn prepare(
        &mut self,
        input: &CompositeExtract,
        targets: &RenderTargets,
        device: &RenderDevice,
        queue: &RenderQueue,
        images: &RenderAssets<GpuImage>,
        cache: &PipelineCache,
    ) {
        self.scene_frame.set(input.frame.clone());
        self.scene_frame.write_buffer(device, queue);
        self.overlay_frame.set(OverlayFrame {
            window_size: input.frame.viewport.window_size,
        });
        self.overlay_frame.write_buffer(device, queue);
        let light_buffer = self.lights.buffer().map(Buffer::id);
        let line_buffer = self.lines.buffer().map(Buffer::id);
        self.lights.set(populated(
            &input.lights,
            PointLight {
                center: Vec2::ZERO,
                radius: 0.0,
                intensity: 0.0,
            },
        ));
        self.lights.write_buffer(device, queue);
        self.lines.set(populated(
            &input.lines,
            LineInstance {
                a: Vec2::ZERO,
                b: Vec2::ZERO,
                color: Vec4::ZERO,
            },
        ));
        self.lines.write_buffer(device, queue);
        let light_buffer_changed = light_buffer != self.lights.buffer().map(Buffer::id);
        let line_buffer_changed = line_buffer != self.lines.buffer().map(Buffer::id);
        let (star_view, star_sampler) = images.get(input.stars.id()).map_or_else(
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
