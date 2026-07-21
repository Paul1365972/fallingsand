use super::targets::RenderTargets;
use super::{color_attachment, pipeline, queue_pipeline};
use bevy::core_pipeline::FullscreenShader;
use bevy::prelude::*;
use bevy::render::render_resource::binding_types::{texture_2d, uniform_buffer};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};

pub const GLOW_RADIUS: f32 = 50.0;
pub const AIR_RADIUS: f32 = 35.0;
pub const LIGHT_MARGIN: u32 = 50;
pub const LIGHT_FIELD_DOWNSCALE: u32 = 4;
const FIELD_TAP_RADIUS: usize = 13;
const FIELD_TAP_COUNT: usize = 2 * FIELD_TAP_RADIUS + 1;
const FIELD_TAP_VEC4S: usize = FIELD_TAP_COUNT.div_ceil(4);

#[derive(Clone, ShaderType)]
struct LightBlurFrame {
    glow_weights: [Vec4; FIELD_TAP_VEC4S],
    air_weights: [Vec4; FIELD_TAP_VEC4S],
}

fn gaussian_kernel_sum(radius: f32) -> f32 {
    let sigma = radius / 3.0;
    let radius = radius.ceil() as i32;
    (-radius..=radius)
        .map(|distance| (-((distance * distance) as f32) / (2.0 * sigma * sigma)).exp())
        .sum()
}

fn field_weights(radius: f32, kernel_sum: f32) -> [Vec4; FIELD_TAP_VEC4S] {
    let sigma = radius / (3.0 * LIGHT_FIELD_DOWNSCALE as f32);
    let taps: [f32; FIELD_TAP_COUNT] = std::array::from_fn(|index| {
        let distance = index as f32 - FIELD_TAP_RADIUS as f32;
        (-(distance * distance) / (2.0 * sigma * sigma)).exp()
    });
    let scale = kernel_sum / taps.iter().sum::<f32>();
    std::array::from_fn(|vector| {
        let tap = |index: usize| taps.get(vector * 4 + index).copied().unwrap_or(0.0);
        Vec4::new(tap(0), tap(1), tap(2), tap(3)) * scale
    })
}

fn blur_frame() -> LightBlurFrame {
    LightBlurFrame {
        glow_weights: field_weights(GLOW_RADIUS, gaussian_kernel_sum(GLOW_RADIUS)),
        air_weights: field_weights(AIR_RADIUS, 1.0),
    }
}

pub fn extended_size(native: UVec2) -> UVec2 {
    UVec2::new(
        (native.x + 2 * LIGHT_MARGIN).next_multiple_of(LIGHT_FIELD_DOWNSCALE),
        (native.y + 2 * LIGHT_MARGIN).next_multiple_of(LIGHT_FIELD_DOWNSCALE),
    )
}

pub fn margin(native: UVec2) -> Vec2 {
    ((extended_size(native) - native) / 2).as_vec2()
}

#[derive(Resource)]
pub(super) struct LightFieldPass {
    downsample_layout: BindGroupLayoutDescriptor,
    blur_layout: BindGroupLayoutDescriptor,
    downsample_pipeline: CachedRenderPipelineId,
    horizontal_pipeline: CachedRenderPipelineId,
    vertical_pipeline: CachedRenderPipelineId,
    blur_frame: UniformBuffer<LightBlurFrame>,
    target_revision: u64,
    downsample_bind_group: Option<BindGroup>,
    horizontal_bind_group: Option<BindGroup>,
    vertical_bind_group: Option<BindGroup>,
}

impl LightFieldPass {
    pub(super) fn new(
        device: &RenderDevice,
        queue: &RenderQueue,
        asset_server: &AssetServer,
        fullscreen: &FullscreenShader,
        cache: &PipelineCache,
    ) -> Self {
        let downsample_layout = BindGroupLayoutDescriptor::new(
            "game_downsample_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (texture_2d(TextureSampleType::Float { filterable: false }),),
            ),
        );
        let blur_layout = BindGroupLayoutDescriptor::new(
            "game_blur_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    uniform_buffer::<LightBlurFrame>(false),
                ),
            ),
        );
        let shader = asset_server.load("shaders/light_field.wgsl");
        let vertex = fullscreen.to_vertex_state();
        let downsample_pipeline = queue_pipeline(
            cache,
            "game_light_downsample_pipeline",
            vec![downsample_layout.clone()],
            vertex.clone(),
            shader.clone(),
            "downsample_fragment",
            None,
        );
        let horizontal_pipeline = queue_pipeline(
            cache,
            "game_light_horizontal_pipeline",
            vec![blur_layout.clone()],
            vertex.clone(),
            shader.clone(),
            "blur_horizontal_fragment",
            None,
        );
        let vertical_pipeline = queue_pipeline(
            cache,
            "game_light_vertical_pipeline",
            vec![blur_layout.clone()],
            vertex,
            shader,
            "blur_vertical_fragment",
            None,
        );
        let mut blur_frame = UniformBuffer::from(blur_frame());
        blur_frame.set_label(Some("game_blur_frame"));
        blur_frame.write_buffer(device, queue);
        Self {
            downsample_layout,
            blur_layout,
            downsample_pipeline,
            horizontal_pipeline,
            vertical_pipeline,
            blur_frame,
            target_revision: u64::MAX,
            downsample_bind_group: None,
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
        let downsample_layout = cache.get_bind_group_layout(&self.downsample_layout);
        self.downsample_bind_group = Some(device.create_bind_group(
            "game_downsample_bind_group",
            &downsample_layout,
            &BindGroupEntries::sequential((&targets.emission.view,)),
        ));
        let blur_layout = cache.get_bind_group_layout(&self.blur_layout);
        let bind_group = |label, source: &TextureView| {
            device.create_bind_group(
                label,
                &blur_layout,
                &BindGroupEntries::sequential((
                    source,
                    self.blur_frame.binding().expect("blur frame written"),
                )),
            )
        };
        self.horizontal_bind_group = Some(bind_group(
            "game_blur_horizontal_bind_group",
            &targets.quarter.view,
        ));
        self.vertical_bind_group = Some(bind_group(
            "game_blur_vertical_bind_group",
            &targets.blur_temp.view,
        ));
        self.target_revision = targets.revision;
    }

    pub(super) fn draw(
        &self,
        context: &mut RenderContext,
        targets: &RenderTargets,
        cache: &PipelineCache,
    ) {
        let Some(downsample) = pipeline(cache, self.downsample_pipeline) else {
            return;
        };
        let Some(horizontal) = pipeline(cache, self.horizontal_pipeline) else {
            return;
        };
        let Some(vertical) = pipeline(cache, self.vertical_pipeline) else {
            return;
        };
        let Some(downsample_bind_group) = self.downsample_bind_group.as_ref() else {
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
                "game_light_downsample_pass",
                &targets.quarter.view,
                downsample,
                downsample_bind_group,
            ),
            (
                "game_light_horizontal_pass",
                &targets.blur_temp.view,
                horizontal,
                horizontal_bind_group,
            ),
            (
                "game_light_vertical_pass",
                &targets.light.view,
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
