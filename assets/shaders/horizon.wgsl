#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct HorizonParams {
    color: vec4<f32>,
    horizon: f32,
    intensity: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: HorizonParams;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let d = in.uv.y - params.horizon;
    let ground = smoothstep(-0.02, 0.02, d);
    let haze = exp(-max(-d, 0.0) * 6.0) * params.intensity;
    let a = clamp(ground + haze * (1.0 - ground), 0.0, 1.0);
    return vec4<f32>(params.color.rgb, a);
}
