#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var cells: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var palette: texture_2d<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dims = textureDimensions(cells);
    let x = min(u32(in.uv.x * f32(dims.x)), dims.x - 1u);
    let y = min(u32((1.0 - in.uv.y) * f32(dims.y)), dims.y - 1u);
    let cell = textureLoad(cells, vec2<u32>(x, y), 0);
    let material = cell.r | (cell.g << 8u);
    let shade = cell.b >> 4u;
    let color = textureLoad(palette, vec2<u32>(material, shade), 0);
    if ((cell.b & 1u) != 0u) {
        let flame = vec3<f32>(1.0, 0.52, 0.14);
        let glow = 0.45 + 0.06 * f32(shade & 3u);
        return vec4<f32>(mix(color.rgb, flame, glow), color.a);
    }
    return color;
}
