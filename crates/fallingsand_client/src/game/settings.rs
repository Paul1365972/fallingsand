#[derive(Clone, Copy, PartialEq, Eq)]
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

#[cfg(not(target_family = "wasm"))]
const SETTINGS_PATH: &str = "saves/settings.txt";

#[cfg(not(target_family = "wasm"))]
pub fn load() -> Settings {
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
pub fn save(settings: &Settings) {
    let _ = std::fs::create_dir_all("saves");
    let content = format!(
        "fullscreen={}\nvsync={}\n",
        settings.fullscreen, settings.vsync
    );
    if let Err(err) = std::fs::write(SETTINGS_PATH, content) {
        bevy::log::warn!("failed to persist settings: {err}");
    }
}

#[cfg(target_family = "wasm")]
pub fn load() -> Settings {
    Settings::default()
}

#[cfg(target_family = "wasm")]
pub fn save(_settings: &Settings) {}
