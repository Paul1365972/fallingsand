#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

const TAP_RADIUS: i32 = 13;
const TAP_VEC4S: u32 = 7u;

struct LightBlurParams {
    glow_weights: array<vec4<f32>, TAP_VEC4S>,
    air_weights: array<vec4<f32>, TAP_VEC4S>,
}

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var<uniform> blur: LightBlurParams;

@fragment
fn downsample_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let out_size = vec2<u32>(textureDimensions(source_tex)) / 4u;
    let p = min(vec2<u32>(floor(in.uv * vec2<f32>(out_size))), out_size - vec2<u32>(1u));
    let base = p * 4u;
    var value = vec4<f32>(0.0);
    for (var y = 0u; y < 4u; y += 1u) {
        for (var x = 0u; x < 4u; x += 1u) {
            value += textureLoad(source_tex, base + vec2<u32>(x, y), 0);
        }
    }
    return value * (1.0 / 16.0);
}

fn blur_tap(center: vec2<i32>, dir: vec2<i32>, distance: i32) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(source_tex));
    let p = clamp(center + dir * distance, vec2<i32>(0), dims - vec2<i32>(1));
    return textureLoad(source_tex, vec2<u32>(p), 0);
}

fn blur_value(uv: vec2<f32>, dir: vec2<i32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(source_tex));
    let center = min(vec2<i32>(floor(uv * vec2<f32>(dims))), dims - vec2<i32>(1));
    var glow = vec3<f32>(0.0);
    var air = 0.0;
    for (var v = 0u; v < TAP_VEC4S; v += 1u) {
        let gw = blur.glow_weights[v];
        let aw = blur.air_weights[v];
        let base = i32(v * 4u) - TAP_RADIUS;
        let s0 = blur_tap(center, dir, base);
        let s1 = blur_tap(center, dir, base + 1);
        let s2 = blur_tap(center, dir, base + 2);
        let s3 = blur_tap(center, dir, base + 3);
        glow += s0.rgb * gw.x + s1.rgb * gw.y + s2.rgb * gw.z + s3.rgb * gw.w;
        air += dot(vec4<f32>(s0.a, s1.a, s2.a, s3.a), aw);
    }
    return vec4<f32>(glow, air);
}

@fragment
fn blur_horizontal_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur_value(in.uv, vec2<i32>(1, 0));
}

@fragment
fn blur_vertical_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur_value(in.uv, vec2<i32>(0, 1));
}
