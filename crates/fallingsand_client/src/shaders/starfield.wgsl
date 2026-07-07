#import bevy_sprite::mesh2d_vertex_output::VertexOutput

const TAU: f32 = 6.2831853;

struct StarfieldParams {
    sidereal: f32,
    aspect: f32,
    star_alpha: f32,
    time: f32,
    horizon: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: StarfieldParams;

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn star_layer(g: vec2<f32>, threshold: f32, time: f32) -> vec3<f32> {
    let cell = floor(g);
    let f = fract(g);
    let present = step(threshold, hash2(cell + vec2<f32>(1.7, 9.2)));
    let pos = vec2<f32>(hash2(cell + vec2<f32>(0.3, 0.0)), hash2(cell + vec2<f32>(0.0, 0.7)));
    let mag = hash2(cell + vec2<f32>(5.1, 2.9));
    let hue = hash2(cell + vec2<f32>(7.2, 1.1));
    let d = length(f - pos);
    let core = 1.0 - smoothstep(0.0, 0.04 + 0.05 * mag, d);
    let twinkle = 0.92 + 0.08 * sin(time * 0.4 + mag * 50.0);
    let tint = mix(vec3<f32>(1.0, 0.9, 0.82), vec3<f32>(0.8, 0.86, 1.0), hue);
    return tint * (core * present * (0.3 + 0.7 * mag * mag) * twinkle);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let c = vec2<f32>((in.uv.x - 0.5) * params.aspect / 1.4, params.horizon - in.uv.y);
    let th = params.sidereal * TAU;
    let ct = cos(th);
    let st = sin(th);
    let q = vec2<f32>(c.x * ct + c.y * st, -c.x * st + c.y * ct);
    var stars = vec3<f32>(0.0);
    stars = stars + star_layer(q * 12.0, 0.82, params.time) * 1.4;
    stars = stars + star_layer(q * 20.0 + vec2<f32>(31.0, 17.0), 0.88, params.time) * 0.9;
    stars = stars + star_layer(q * 32.0 + vec2<f32>(53.0, 61.0), 0.92, params.time) * 0.55;

    let above = 1.0 - smoothstep(params.horizon - 0.04, params.horizon, in.uv.y);
    let alpha = clamp(max(max(stars.r, stars.g), stars.b), 0.0, 1.0) * params.star_alpha * above;
    return vec4<f32>(stars, alpha);
}
