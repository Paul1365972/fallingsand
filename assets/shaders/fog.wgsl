#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

const TAP_RADIUS: i32 = 18;
const TAP_VEC4S: u32 = 10u;

struct FogBlurFrame {
    weights: array<vec4<f32>, TAP_VEC4S>,
}

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var<uniform> frame: FogBlurFrame;

fn blur_tap(center: vec2<i32>, dir: vec2<i32>, distance: i32) -> f32 {
    let dims = vec2<i32>(textureDimensions(source_tex));
    let p = clamp(center + dir * distance, vec2<i32>(0), dims - vec2<i32>(1));
    return textureLoad(source_tex, vec2<u32>(p), 0).r;
}

fn blur_value(uv: vec2<f32>, dir: vec2<i32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(source_tex));
    let center = min(vec2<i32>(floor(uv * vec2<f32>(dims))), dims - vec2<i32>(1));
    var total = 0.0;
    for (var v = 0u; v < TAP_VEC4S; v += 1u) {
        let weights = frame.weights[v];
        let base = i32(v * 4u) - TAP_RADIUS;
        total += blur_tap(center, dir, base) * weights.x;
        total += blur_tap(center, dir, base + 1) * weights.y;
        total += blur_tap(center, dir, base + 2) * weights.z;
        total += blur_tap(center, dir, base + 3) * weights.w;
    }
    return total;
}

@fragment
fn blur_horizontal_fragment(in: FullscreenVertexOutput) -> @location(0) f32 {
    return blur_value(in.uv, vec2<i32>(1, 0));
}

@fragment
fn blur_vertical_fragment(in: FullscreenVertexOutput) -> @location(0) f32 {
    return blur_value(in.uv, vec2<i32>(0, 1));
}
