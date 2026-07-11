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
    return textureLoad(palette, vec2<u32>(material, shade), 0);
}
