#import fallingsand::common::{PixelViewport, vnoise}

const CHUNK_SIZE: f32 = 64.0;

struct RasterFrame {
    viewport: PixelViewport,
    world_snapped: vec2<f32>,
    emission_size: vec2<f32>,
    time: f32,
}

struct ChunkInstance {
    world_origin: vec2<f32>,
    atlas_origin: vec2<u32>,
}

struct QuadInstance {
    center: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: RasterFrame;
@group(0) @binding(1) var<storage, read> chunks: array<ChunkInstance>;
@group(0) @binding(2) var<storage, read> quads: array<QuadInstance>;
@group(0) @binding(3) var atlas: texture_2d<u32>;
@group(0) @binding(4) var palette: texture_2d<f32>;
@group(0) @binding(5) var emissive_palette: texture_2d<f32>;

struct RasterOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) atlas_position: vec2<f32>,
    @location(1) world_position: vec2<f32>,
}

struct ColorOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

fn quad_corner(vertex: u32) -> vec2<f32> {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0)
    );
    return corners[vertex];
}

fn world_clip(world: vec2<f32>, size: vec2<f32>) -> vec4<f32> {
    let relative = world - frame.world_snapped;
    return vec4<f32>(relative.x * 2.0 / size.x, relative.y * 2.0 / size.y, 0.0, 1.0);
}

@vertex
fn chunk_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> RasterOutput {
    let corner = quad_corner(vertex);
    let item = chunks[instance];
    let world = item.world_origin + corner * CHUNK_SIZE;
    var out: RasterOutput;
    out.clip_position = world_clip(world, frame.viewport.native_size);
    out.atlas_position = vec2<f32>(item.atlas_origin) + corner * CHUNK_SIZE;
    out.world_position = world;
    return out;
}

@vertex
fn emissive_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> RasterOutput {
    let corner = quad_corner(vertex);
    let item = chunks[instance];
    let world = item.world_origin + corner * CHUNK_SIZE;
    var out: RasterOutput;
    out.clip_position = world_clip(world, frame.emission_size);
    out.atlas_position = vec2<f32>(item.atlas_origin) + corner * CHUNK_SIZE;
    out.world_position = world;
    return out;
}

fn cell_entry(position: vec2<f32>) -> vec4<u32> {
    let dims = vec2<u32>(textureDimensions(atlas));
    let p = min(vec2<u32>(position), dims - vec2<u32>(1u));
    return textureLoad(atlas, p, 0);
}

@fragment
fn chunk_fragment(in: RasterOutput) -> @location(0) vec4<f32> {
    let cell = cell_entry(in.atlas_position);
    let material = cell.r | (cell.g << 8u);
    return textureLoad(palette, vec2<u32>(material, cell.b & 15u), 0);
}

@fragment
fn emissive_fragment(in: RasterOutput) -> @location(0) vec4<f32> {
    let cell = cell_entry(in.atlas_position);
    let material = cell.r | (cell.g << 8u);
    let shade = cell.b & 15u;
    let entry = textureLoad(emissive_palette, vec2<u32>(material, shade), 0);
    var emission = entry.rgb;
    if entry.a > 0.0 {
        let coarse = vnoise(in.world_position * (1.0 / 18.0) + vec2<f32>(0.0, -frame.time * 0.9));
        let fine = vnoise(in.world_position * (1.0 / 6.0) + vec2<f32>(0.0, -frame.time * 1.9));
        let n = mix(coarse, fine, 0.35) * 2.0 - 1.0;
        emission *= max(0.0, 1.0 + entry.a * n);
    }
    let air = 1.0 - textureLoad(palette, vec2<u32>(material, shade), 0).a;
    return vec4<f32>(emission, air);
}

@vertex
fn quad_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> ColorOutput {
    let corner = quad_corner(vertex);
    let item = quads[instance];
    let world = item.center + (corner - vec2<f32>(0.5)) * item.size;
    var out: ColorOutput;
    out.clip_position = world_clip(world, frame.viewport.native_size);
    out.color = item.color;
    return out;
}

@fragment
fn quad_fragment(in: ColorOutput) -> @location(0) vec4<f32> {
    return in.color;
}
