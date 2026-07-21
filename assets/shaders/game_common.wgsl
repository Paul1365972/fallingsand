#define_import_path fallingsand::game_common

struct PixelViewport {
    native_size: vec2<f32>,
    window_size: vec2<f32>,
    physical_size: vec2<f32>,
    window_center: vec2<f32>,
}

fn pcg(v: u32) -> u32 {
    var x = v * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

fn cell_hash(cell: vec2<i32>) -> f32 {
    let h = pcg(bitcast<u32>(cell.x) * 1597334677u ^ bitcast<u32>(cell.y) * 3812015801u);
    return f32(h) / 4294967295.0;
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = cell_hash(i);
    let b = cell_hash(i + vec2<i32>(1, 0));
    let c = cell_hash(i + vec2<i32>(0, 1));
    let d = cell_hash(i + vec2<i32>(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn layer_uv(pixel: vec2<f32>, offset: vec2<f32>, viewport: PixelViewport) -> vec2<f32> {
    let screen_offset = vec2<f32>(-offset.x, offset.y);
    return (pixel + screen_offset + (viewport.physical_size - viewport.window_size) * 0.5) / viewport.physical_size;
}

fn layer_texel(uv: vec2<f32>, viewport: PixelViewport) -> vec2<f32> {
    return min(floor(uv * viewport.native_size), viewport.native_size - vec2<f32>(1.0));
}

fn layer_cell(texel: vec2<f32>, snapped: vec2<f32>, viewport: PixelViewport) -> vec2<f32> {
    return snapped + vec2<f32>(
        texel.x + 0.5 - viewport.native_size.x * 0.5,
        viewport.native_size.y * 0.5 - texel.y - 0.5
    );
}

fn composite_over_opaque(dst: vec3<f32>, src_premultiplied: vec4<f32>) -> vec3<f32> {
    return src_premultiplied.rgb + dst * (1.0 - src_premultiplied.a);
}
