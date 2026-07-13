#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::celestial::{position_hash, snapped_disc_position, disc_coverage, aura_falloff, unpremultiply}

struct SunParams {
    redness: f32,
    occlusion: f32,
    quad_size: f32,
    disc_radius: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: SunParams;

const DISC_HDR: f32 = 14.0;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let quad_size = max(params.quad_size, 1.0);
    let position = snapped_disc_position(in.uv, quad_size);
    let pixel_size = 2.0 / quad_size;
    let radius = length(position);
    let coverage = disc_coverage(radius, params.disc_radius, pixel_size);
    let normalized_radius = clamp(radius / params.disc_radius, 0.0, 1.0);

    let core_color = vec3<f32>(1.0, 0.88, 0.59);
    let mid_color = vec3<f32>(1.0, 0.64, 0.17);
    let edge_color = vec3<f32>(1.0, 0.22, 0.03);
    var disc_color = mix(core_color, mid_color, smoothstep(0.0, 0.5, normalized_radius));
    disc_color = mix(disc_color, edge_color, smoothstep(0.5, 1.0, normalized_radius));

    let surface_grain = (position_hash(floor(in.uv * quad_size)) - 0.5) * 0.05;
    disc_color *= 1.0 + surface_grain * (1.0 - normalized_radius);

    let sunset_color = disc_color * vec3<f32>(1.0, 0.74, 0.46);
    let hdr_disc_color = mix(disc_color, sunset_color, params.redness) * DISC_HDR;

    let aura_strength = aura_falloff(radius, params.disc_radius) * (1.0 - coverage);
    let aura_color = mix(vec3<f32>(1.0, 0.62, 0.30), vec3<f32>(1.0, 0.40, 0.17), params.redness) * 1.6;

    let eclipse_strength = params.occlusion * params.occlusion;
    let corona = pow(clamp(1.0 - radius, 0.0, 1.0), 2.2) * 2.4 * eclipse_strength;
    let corona_color = vec3<f32>(1.0, 0.82, 0.52) * 3.0;

    let aura_alpha = aura_strength * 0.5;
    let alpha = coverage + aura_alpha + corona;
    let premultiplied_color = hdr_disc_color * coverage
        + aura_color * aura_alpha
        + corona_color * corona;
    return unpremultiply(premultiplied_color, alpha);
}
