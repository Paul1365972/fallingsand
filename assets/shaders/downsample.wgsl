#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import fallingsand::layer_common::layer_texel

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var src: texture_2d<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let out_dims = vec2<f32>(textureDimensions(src) / 2u);
    let base = vec2<u32>(layer_texel(in.uv, out_dims)) * 2u;
    let a = textureLoad(src, base, 0);
    let b = textureLoad(src, base + vec2<u32>(1u, 0u), 0);
    let c = textureLoad(src, base + vec2<u32>(0u, 1u), 0);
    let d = textureLoad(src, base + vec2<u32>(1u, 1u), 0);
    return (a + b + c + d) * 0.25;
}
