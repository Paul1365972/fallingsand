use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::WindowCreated;
use bevy::winit::WINIT_WINDOWS;

const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon.png");

pub struct IconPlugin;

impl Plugin for IconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_window_icons);
    }
}

fn set_window_icons(mut created: MessageReader<WindowCreated>, _main_thread: NonSendMarker) {
    if created.is_empty() {
        return;
    }
    let rgba = image::load_from_memory(ICON_PNG)
        .expect("assets/icon.png must be a valid PNG")
        .into_rgba8();
    let (width, height) = rgba.dimensions();
    let icon = winit::window::Icon::from_rgba(rgba.into_raw(), width, height)
        .expect("icon dimensions must match its pixel data");
    WINIT_WINDOWS.with_borrow(|windows| {
        for message in created.read() {
            if let Some(window) = windows.get_window(message.window) {
                window.set_window_icon(Some(icon.clone()));
            }
        }
    });
}
