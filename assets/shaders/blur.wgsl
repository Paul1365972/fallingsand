#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::layer_common::layer_texel

struct BlurParams {
    dir: vec2<f32>,
    radius: f32,
    _pad: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: BlurParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var src: texture_2d<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(src));
    let center = vec2<i32>(layer_texel(in.uv, vec2<f32>(dims)));
    let dir = vec2<i32>(params.dir);
    let r = i32(params.radius);
    var acc = vec3<f32>(0.0);
    for (var d = -r; d <= r; d = d + 1) {
        let f = 1.0 - abs(f32(d)) / (params.radius + 1.0);
        let p = clamp(center + dir * d, vec2<i32>(0), dims - vec2<i32>(1));
        let s = textureLoad(src, vec2<u32>(p), 0).rgb;
        acc = max(acc, s * f);
    }
    return vec4<f32>(acc, 1.0);
}
