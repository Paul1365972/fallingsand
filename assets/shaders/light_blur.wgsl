#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::layer_common::layer_texel

const TAP_RADIUS: i32 = 13;
const TAP_VEC4S: u32 = 7u;

struct LightBlurParams {
    glow_weights: array<vec4<f32>, TAP_VEC4S>,
    air_weights: array<vec4<f32>, TAP_VEC4S>,
    dir: vec2<f32>,
    _pad: vec2<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: LightBlurParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var src: texture_2d<f32>;

fn tap(center: vec2<i32>, dir: vec2<i32>, d: i32, dims: vec2<i32>) -> vec4<f32> {
    let p = clamp(center + dir * d, vec2<i32>(0), dims - vec2<i32>(1));
    return textureLoad(src, vec2<u32>(p), 0);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(src));
    let center = vec2<i32>(layer_texel(in.uv, vec2<f32>(dims)));
    let dir = vec2<i32>(params.dir);
    var glow = vec3<f32>(0.0);
    var air = 0.0;
    for (var v = 0u; v < TAP_VEC4S; v = v + 1u) {
        let gw = params.glow_weights[v];
        let aw = params.air_weights[v];
        let base = i32(v * 4u) - TAP_RADIUS;
        let s0 = tap(center, dir, base, dims);
        let s1 = tap(center, dir, base + 1, dims);
        let s2 = tap(center, dir, base + 2, dims);
        let s3 = tap(center, dir, base + 3, dims);
        glow = glow + s0.rgb * gw.x + s1.rgb * gw.y + s2.rgb * gw.z + s3.rgb * gw.w;
        air = air + dot(vec4<f32>(s0.a, s1.a, s2.a, s3.a), aw);
    }
    return vec4<f32>(glow, air);
}
