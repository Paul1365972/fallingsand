#define_import_path fallingsand::celestial

fn position_hash(position: vec2<f32>) -> f32 {
    return fract(sin(dot(position, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn snapped_disc_position(uv: vec2<f32>, quad_size: f32) -> vec2<f32> {
    let snapped_uv = (floor(uv * quad_size) + vec2<f32>(0.5)) / quad_size;
    return vec2<f32>(snapped_uv.x - 0.5, 0.5 - snapped_uv.y) * 2.0;
}

fn disc_coverage(radius: f32, disc_radius: f32, pixel_size: f32) -> f32 {
    return clamp((disc_radius - radius) / pixel_size + 0.5, 0.0, 1.0);
}

fn aura_falloff(radius: f32, disc_radius: f32) -> f32 {
    let strength = clamp((1.0 - radius) / max(1.0 - disc_radius, 1e-4), 0.0, 1.0);
    return strength * strength;
}

fn quantize(x: f32, levels: f32) -> f32 {
    return floor(clamp(x, 0.0, 1.0) * levels + 0.5) / levels;
}

fn unpremultiply(premultiplied_color: vec3<f32>, alpha: f32) -> vec4<f32> {
    let clamped_alpha = clamp(alpha, 0.0, 1.0);
    return vec4<f32>(
        premultiplied_color / max(clamped_alpha, 1e-4),
        clamped_alpha
    );
}
