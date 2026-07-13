#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct SunParams {
    redness: f32,
    occlusion: f32,
    _pad: vec2<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: SunParams;

const GRID_SIZE: f32 = 48.0;
const DISC_RADIUS: f32 = 0.91;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = floor(in.uv * GRID_SIZE);
    let grid = (texel + vec2<f32>(0.5)) / GRID_SIZE;
    let p = (grid - vec2<f32>(0.5)) * 2.0;
    let pixel = 2.0 / GRID_SIZE;
    let radius = length(p);
    let cover = clamp((DISC_RADIUS - radius) / pixel + 0.5, 0.0, 1.0);

    let inward = clamp(1.0 - radius / DISC_RADIUS, 0.0, 1.0);
    let core = pow(inward, 8.0);
    let ring = floor(core * 10.0 + 0.5) / 10.0;
    let orange = vec3<f32>(1.0, 0.216, 0.027);
    let pale_core = vec3<f32>(1.0, 0.880, 0.591);
    var surface = mix(orange, pale_core, ring);
    let granule = floor(hash(texel) * 3.0) - 1.0;
    surface *= 1.0 + granule * 0.025 * (1.0 - ring);

    let warm = surface * vec3<f32>(1.0, 0.58, 0.32);
    var color = mix(surface, warm, params.redness) * 6.5;

    let eclipse = params.occlusion * params.occlusion;
    let halo = pow(clamp(1.0 - radius, 0.0, 1.0), 2.2) * 2.6 * eclipse;
    color += vec3<f32>(1.0, 0.72, 0.32) * halo;

    return vec4<f32>(color, clamp(cover + halo, 0.0, 1.0));
}
