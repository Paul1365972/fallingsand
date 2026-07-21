#define_import_path fallingsand::game_world
#import fallingsand::game_common::{layer_cell, layer_texel, layer_uv, pcg, vnoise}
#import fallingsand::game_scene_bindings::{SilhouetteParams}
#import fallingsand::game_scene_bindings as bindings

const LIGHT_FIELD_DOWNSCALE: f32 = 4.0;
const CAVE_DARK: vec3<f32> = vec3<f32>(0.01, 0.012, 0.03);

fn silhouette_hash(position: f32, seed: f32) -> f32 {
    let value = pcg(bitcast<u32>(i32(position)) ^ (u32(seed) * 374761393u));
    return f32(value) / 4294967296.0;
}

fn silhouette_noise(position: f32, seed: f32) -> f32 {
    let integer = floor(position);
    let fraction = position - integer;
    let blend = fraction * fraction * (3.0 - 2.0 * fraction);
    return mix(
        silhouette_hash(integer, seed),
        silhouette_hash(integer + 1.0, seed),
        blend,
    );
}

fn silhouette_fbm(position: f32, seed: f32) -> f32 {
    return silhouette_noise(position, seed) * 0.55
        + silhouette_noise(position * 2.03 + 13.7, seed) * 0.3
        + silhouette_noise(position * 4.01 + 41.3, seed) * 0.15;
}

fn silhouette_height(cell_x: f32, params: SilhouetteParams) -> f32 {
    let noise = silhouette_fbm(cell_x * params.inv_wavelength, params.seed);
    return params.base + params.amp * (noise * 2.0 - 1.0);
}

fn silhouette_layer_premultiplied(
    uv: vec2<f32>,
    params: SilhouetteParams,
) -> vec4<f32> {
    let viewport = bindings::frame.viewport;
    let cell = layer_cell(layer_texel(uv, viewport), params.snapped_cam, viewport);
    let alpha = select(
        0.0,
        params.color.a,
        cell.y < silhouette_height(cell.x, params),
    );
    return vec4<f32>(params.color.rgb * alpha, alpha);
}

fn far_silhouette_layer_premultiplied(uv: vec2<f32>) -> vec4<f32> {
    return silhouette_layer_premultiplied(uv, bindings::frame.world.far);
}

fn near_silhouette_layer_premultiplied(uv: vec2<f32>) -> vec4<f32> {
    return silhouette_layer_premultiplied(uv, bindings::frame.world.near);
}

fn point_glow(position: vec2<f32>) -> f32 {
    var result = 0.0;
    for (var i = 0u; i < bindings::frame.world.lighting.light_count; i += 1u) {
        let light = bindings::frame.world.lighting.lights[i];
        let falloff = clamp(
            1.0
                - distance(position, floor(light.xy) + vec2<f32>(0.5))
                    / max(light.z, 1.0),
            0.0,
            1.0,
        );
        result = max(result, falloff * falloff * light.w);
    }
    return result;
}

fn wall_layer_premultiplied(uv: vec2<f32>) -> vec4<f32> {
    let viewport = bindings::frame.viewport;
    let cell = layer_cell(
        layer_texel(uv, viewport),
        bindings::frame.world.wall_snapped,
        viewport,
    );
    let world = cell + bindings::frame.world.wall.world_offset;
    let n = vnoise(cell * 0.11) * 0.55
        + vnoise(cell * 0.2233 + vec2<f32>(13.7, 41.3)) * 0.3
        + vnoise(cell * 0.4411 + vec2<f32>(71.9, 7.5)) * 0.15;
    let step_value = min(u32(n * 4.0), 3u);
    var rgb = bindings::frame.world.wall.base_color.rgb
        * (0.7 + 0.15 * f32(step_value));
    rgb = mix(
        rgb,
        CAVE_DARK,
        clamp(
            bindings::frame.world.lighting.darkness * (1.0 - point_glow(world)),
            0.0,
            1.0,
        ),
    );
    let alpha = (1.0 - smoothstep(-24.0, 8.0, world.y))
        * bindings::frame.world.wall.base_color.a;
    return vec4<f32>(rgb * alpha, alpha);
}

fn lit_world_premultiplied(pixel: vec2<f32>) -> vec4<f32> {
    let viewport = bindings::frame.viewport;
    let uv = layer_uv(pixel, bindings::frame.world_offset, viewport);
    let t = layer_texel(uv, viewport);
    let p = vec2<u32>(t);
    let world = textureLoad(bindings::world_tex, p, 0);
    let cell = layer_cell(
        t,
        bindings::frame.world.lighting.snapped_cam,
        viewport,
    );
    let te = t + bindings::frame.world.lighting.margin;
    let core = textureLoad(bindings::emission_tex, vec2<u32>(te), 0).rgb;
    let field_size = vec2<f32>(textureDimensions(bindings::light_tex))
        * LIGHT_FIELD_DOWNSCALE;
    let field = textureSample(
        bindings::light_tex,
        bindings::linear_sampler,
        (te + vec2<f32>(0.5)) / field_size,
    );
    let halo = field.rgb;
    let lit = max(point_glow(cell), max(halo.r, max(halo.g, halo.b)) * 0.1);
    let ambient = clamp(field.a * 10.0, 0.0, 1.0)
        * (1.0 - bindings::frame.world.lighting.darkness);
    let incident = clamp(ambient + lit, 0.0, 1.0);
    var rgb = mix(CAVE_DARK * world.a, world.rgb, incident);
    rgb += halo * (world.a * 0.03) + core * (world.a * 3.0);
    return vec4<f32>(rgb, world.a);
}
