use bevy::prelude::*;

pub struct SettingsPlugin;

#[derive(Resource, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub fullscreen: bool,
    pub vsync: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            vsync: true,
        }
    }
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load());
        app.add_systems(Update, fullscreen_hotkey);
        #[cfg(not(target_family = "wasm"))]
        app.add_systems(Update, apply.run_if(resource_changed::<Settings>));
    }
}

fn fullscreen_hotkey(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>) {
    if keys.just_pressed(KeyCode::F11) {
        settings.fullscreen = !settings.fullscreen;
    }
}

#[cfg(not(target_family = "wasm"))]
fn apply(
    settings: Res<Settings>,
    mut window: Single<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    use bevy::window::{MonitorSelection, PresentMode, WindowMode};
    window.mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    window.present_mode = if settings.vsync {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
    save(&settings);
}

#[cfg(not(target_family = "wasm"))]
const SETTINGS_PATH: &str = "saves/settings.txt";

#[cfg(not(target_family = "wasm"))]
fn load() -> Settings {
    let mut settings = Settings::default();
    let Ok(text) = std::fs::read_to_string(SETTINGS_PATH) else {
        return settings;
    };
    for line in text.lines() {
        match line.trim() {
            "fullscreen=true" => settings.fullscreen = true,
            "fullscreen=false" => settings.fullscreen = false,
            "vsync=true" => settings.vsync = true,
            "vsync=false" => settings.vsync = false,
            _ => {}
        }
    }
    settings
}

#[cfg(not(target_family = "wasm"))]
fn save(settings: &Settings) {
    let _ = std::fs::create_dir_all("saves");
    let content = format!(
        "fullscreen={}\nvsync={}\n",
        settings.fullscreen, settings.vsync
    );
    if let Err(err) = std::fs::write(SETTINGS_PATH, content) {
        warn!("failed to persist settings: {err}");
    }
}

#[cfg(target_family = "wasm")]
fn load() -> Settings {
    Settings::default()
}
