#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct SunParams {
    redness: f32,
    occlusion: f32,
    _pad: vec2<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: SunParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var tex_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(tex, tex_sampler, in.uv);
    let p = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let r = length(p);

    let warm = sample.rgb * vec3<f32>(1.0, 0.58, 0.32);
    var col = mix(sample.rgb, warm, params.redness) * 14.0;

    let corona = 2.6 * params.occlusion * params.occlusion;
    let halo = pow(clamp(1.0 - r, 0.0, 1.0), 2.2) * corona;
    col = col + vec3<f32>(1.0, 0.95, 0.90) * halo;

    let alpha = clamp(sample.a + halo, 0.0, 1.0);
    return vec4<f32>(col, alpha);
}
