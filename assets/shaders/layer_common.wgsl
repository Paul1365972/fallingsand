#define_import_path fallingsand::layer_common

fn layer_texel(uv: vec2<f32>, native: vec2<f32>) -> vec2<f32> {
    return min(floor(uv * native), native - vec2<f32>(1.0));
}

fn layer_cell(t: vec2<f32>, snapped_cam: vec2<f32>, native: vec2<f32>) -> vec2<f32> {
    return snapped_cam + vec2<f32>(t.x + 0.5 - native.x * 0.5, native.y * 0.5 - t.y - 0.5);
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
