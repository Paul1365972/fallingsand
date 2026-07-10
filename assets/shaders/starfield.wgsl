#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct StarfieldParams {
    tiling: f32,
    aspect: f32,
    star_visibility: f32,
    horizon: f32,
    time: f32,
    scroll: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: StarfieldParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var tex_sampler: sampler;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(in.uv.x * params.tiling + params.scroll, in.uv.y * params.tiling / params.aspect);
    let sample = textureSample(tex, tex_sampler, uv);

    let cell = floor(fract(uv) * 512.0);
    let phase = hash(cell) * 6.2831853;
    let rate = 1.0 + 2.5 * hash(cell + vec2<f32>(19.3, 7.1));
    let flicker = 1.0 - 0.55 * pow(0.5 + 0.5 * sin(params.time * rate + phase), 2.0);

    let magnitude = hash(cell + vec2<f32>(41.7, 289.3));
    let vis = smoothstep(0.0, 0.12, params.star_visibility - magnitude * 0.85);

    let above = 1.0 - smoothstep(params.horizon - 0.04, params.horizon, in.uv.y);
    let alpha = clamp(sample.a * vis * above * flicker, 0.0, 1.0);
    return vec4<f32>(sample.rgb, alpha);
}
