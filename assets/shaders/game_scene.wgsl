#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import fallingsand::game_backdrop::backdrop_color
#import fallingsand::game_scene_bindings as bindings
#import fallingsand::game_world::lit_world_premultiplied

@fragment
fn scene_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let pixel = in.uv * bindings::frame.viewport.window_size;
    var backdrop = bindings::frame.clear_color.rgb;
    if bindings::frame.backdrop_ready != 0u {
        backdrop = backdrop_color(pixel);
    }
    let world = lit_world_premultiplied(pixel);
    return vec4<f32>(world.rgb + backdrop * (1.0 - world.a), 1.0);
}
