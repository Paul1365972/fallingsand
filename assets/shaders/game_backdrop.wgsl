#define_import_path fallingsand::game_backdrop
#import fallingsand::game_celestial::{
    atmosphere_layer_premultiplied,
    moon_layer_premultiplied,
    star_layer_premultiplied,
    sun_layer_premultiplied,
}
#import fallingsand::game_common::{composite_over_opaque, layer_uv}
#import fallingsand::game_scene_bindings as bindings
#import fallingsand::game_world::{
    far_silhouette_layer_premultiplied,
    near_silhouette_layer_premultiplied,
    wall_layer_premultiplied,
}

fn backdrop_uv(pixel: vec2<f32>, offset: vec2<f32>) -> vec2<f32> {
    return layer_uv(pixel, offset, bindings::frame.viewport);
}

fn backdrop_color(pixel: vec2<f32>) -> vec3<f32> {
    var color = bindings::frame.clear_color.rgb;
    let star_uv = backdrop_uv(pixel, bindings::frame.backdrop.star_offset);
    let star = star_layer_premultiplied(star_uv);
    color = composite_over_opaque(color, star);

    let sun = sun_layer_premultiplied(pixel);
    color = composite_over_opaque(color, sun);

    let moon = moon_layer_premultiplied(pixel);
    color = composite_over_opaque(color, moon);

    let atmosphere_uv = backdrop_uv(pixel, vec2<f32>(0.0));
    let atmosphere = atmosphere_layer_premultiplied(atmosphere_uv);
    color = composite_over_opaque(color, atmosphere);

    let far_uv = backdrop_uv(pixel, bindings::frame.backdrop.far_offset);
    let far = far_silhouette_layer_premultiplied(far_uv);
    color = composite_over_opaque(color, far);

    let near_uv = backdrop_uv(pixel, bindings::frame.backdrop.near_offset);
    let near = near_silhouette_layer_premultiplied(near_uv);
    color = composite_over_opaque(color, near);

    let wall_uv = backdrop_uv(pixel, bindings::frame.backdrop.wall_offset);
    let wall = wall_layer_premultiplied(wall_uv);
    return composite_over_opaque(color, wall);
}
