#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct SunParams {
    redness: f32,
    occlusion: f32,
    _pad: vec2<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: SunParams;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let r = length(p);

    let core_col = mix(vec3<f32>(1.0, 0.96, 0.82), vec3<f32>(1.0, 0.52, 0.22), params.redness);
    let glow_col = mix(core_col, vec3<f32>(1.0, 0.98, 0.92), 0.6);

    let disc = 1.0 - smoothstep(0.32, 0.38, r);
    let corona = 0.6 + 2.6 * params.occlusion * params.occlusion;
    let halo = pow(clamp(1.0 - r, 0.0, 1.0), 2.2) * corona;

    let col = core_col * 16.0 * disc + glow_col * 2.2 * halo;
    let alpha = clamp(disc + halo, 0.0, 1.0);
    return vec4<f32>(col, alpha);
}
