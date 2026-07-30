use super::targets::{FOG_FORMAT, RenderTargets};
use super::{color_attachment, pipeline, queue_pipeline};
use bevy::core_pipeline::FullscreenShader;
use bevy::prelude::*;
use bevy::render::render_resource::binding_types::{texture_2d, uniform_buffer};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};

const BLUR_SIGMA: f32 = 6.0;
const TAP_RADIUS: usize = 18;
const TAP_COUNT: usize = 2 * TAP_RADIUS + 1;
const TAP_VEC4S: usize = TAP_COUNT.div_ceil(4);

#[derive(Clone, ShaderType)]
struct FogBlurFrame {
    weights: [Vec4; TAP_VEC4S],
}

fn blur_frame() -> FogBlurFrame {
    let taps: [f32; TAP_COUNT] = std::array::from_fn(|index| {
        let distance = index as f32 - TAP_RADIUS as f32;
        (-(distance * distance) / (2.0 * BLUR_SIGMA * BLUR_SIGMA)).exp()
    });
    let scale = 1.0 / taps.iter().sum::<f32>();
    FogBlurFrame {
        weights: std::array::from_fn(|vector| {
            let tap = |index: usize| taps.get(vector * 4 + index).copied().unwrap_or(0.0);
            Vec4::new(tap(0), tap(1), tap(2), tap(3)) * scale
        }),
    }
}

#[derive(Resource)]
pub(super) struct FogFieldPass {
    layout: BindGroupLayoutDescriptor,
    horizontal_pipeline: CachedRenderPipelineId,
    vertical_pipeline: CachedRenderPipelineId,
    blur_frame: UniformBuffer<FogBlurFrame>,
    target_revision: u64,
    horizontal_bind_group: Option<BindGroup>,
    vertical_bind_group: Option<BindGroup>,
}

impl FogFieldPass {
    pub(super) fn new(
        device: &RenderDevice,
        queue: &RenderQueue,
        asset_server: &AssetServer,
        fullscreen: &FullscreenShader,
        cache: &PipelineCache,
    ) -> Self {
        let layout = BindGroupLayoutDescriptor::new(
            "game_fog_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    uniform_buffer::<FogBlurFrame>(false),
                ),
            ),
        );
        let shader = asset_server.load("shaders/fog.wgsl");
        let vertex = fullscreen.to_vertex_state();
        let horizontal_pipeline = queue_pipeline(
            cache,
            "game_fog_horizontal_pipeline",
            vec![layout.clone()],
            vertex.clone(),
            shader.clone(),
            "blur_horizontal_fragment",
            &[(FOG_FORMAT, None)],
        );
        let vertical_pipeline = queue_pipeline(
            cache,
            "game_fog_vertical_pipeline",
            vec![layout.clone()],
            vertex,
            shader,
            "blur_vertical_fragment",
            &[(FOG_FORMAT, None)],
        );
        let mut blur_frame = UniformBuffer::from(blur_frame());
        blur_frame.set_label(Some("game_fog_blur_frame"));
        blur_frame.write_buffer(device, queue);
        Self {
            layout,
            horizontal_pipeline,
            vertical_pipeline,
            blur_frame,
            target_revision: u64::MAX,
            horizontal_bind_group: None,
            vertical_bind_group: None,
        }
    }

    pub(super) fn prepare(
        &mut self,
        targets: &RenderTargets,
        device: &RenderDevice,
        cache: &PipelineCache,
    ) {
        if self.target_revision == targets.revision {
            return;
        }
        let layout = cache.get_bind_group_layout(&self.layout);
        let bind_group = |label, source: &TextureView| {
            device.create_bind_group(
                label,
                &layout,
                &BindGroupEntries::sequential((
                    source,
                    self.blur_frame.binding().expect("fog blur frame written"),
                )),
            )
        };
        self.horizontal_bind_group = Some(bind_group(
            "game_fog_horizontal_bind_group",
            &targets.fog_source.view,
        ));
        self.vertical_bind_group = Some(bind_group(
            "game_fog_vertical_bind_group",
            &targets.fog_temp.view,
        ));
        self.target_revision = targets.revision;
    }

    pub(super) fn draw(
        &self,
        context: &mut RenderContext,
        targets: &RenderTargets,
        cache: &PipelineCache,
    ) {
        let Some(horizontal) = pipeline(cache, self.horizontal_pipeline) else {
            return;
        };
        let Some(vertical) = pipeline(cache, self.vertical_pipeline) else {
            return;
        };
        let Some(horizontal_bind_group) = self.horizontal_bind_group.as_ref() else {
            return;
        };
        let Some(vertical_bind_group) = self.vertical_bind_group.as_ref() else {
            return;
        };
        for (label, target, pipeline, bind_group) in [
            (
                "game_fog_horizontal_pass",
                &targets.fog_temp.view,
                horizontal,
                horizontal_bind_group,
            ),
            (
                "game_fog_vertical_pass",
                &targets.fog.view,
                vertical,
                vertical_bind_group,
            ),
        ] {
            let mut pass = context
                .command_encoder()
                .begin_render_pass(&RenderPassDescriptor {
                    label: Some(label),
                    color_attachments: &[Some(color_attachment(target, Some(Color::NONE)))],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
