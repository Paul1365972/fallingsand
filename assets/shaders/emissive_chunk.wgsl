#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals
#import fallingsand::layer_common::vnoise

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var cells: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var emissive_palette: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var palette: texture_2d<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dims = textureDimensions(cells);
    let x = min(u32(in.uv.x * f32(dims.x)), dims.x - 1u);
    let y = min(u32((1.0 - in.uv.y) * f32(dims.y)), dims.y - 1u);
    let cell = textureLoad(cells, vec2<u32>(x, y), 0);
    let material = cell.r | (cell.g << 8u);
    let shade = cell.b & 15u;
    let entry = textureLoad(emissive_palette, vec2<u32>(material, shade), 0);
    var emission = entry.rgb;
    let flicker = entry.a;
    if flicker > 0.0 {
        let world = in.world_position.xy;
        let t = globals.time;
        let coarse = vnoise(world * (1.0 / 18.0) + vec2<f32>(0.0, -t * 0.9));
        let fine = vnoise(world * (1.0 / 6.0) + vec2<f32>(0.0, -t * 1.9));
        let n = mix(coarse, fine, 0.35) * 2.0 - 1.0;
        emission = emission * max(0.0, 1.0 + flicker * n);
    }
    let air = 1.0 - textureLoad(palette, vec2<u32>(material, shade), 0).a;
    return vec4<f32>(emission, air);
}
