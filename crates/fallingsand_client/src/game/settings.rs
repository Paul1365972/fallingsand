#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    #[default]
    PixelPerfect,
    Smooth,
    Retro,
}

impl RenderMode {
    pub fn label(self) -> &'static str {
        match self {
            RenderMode::PixelPerfect => "pixel-perfect",
            RenderMode::Smooth => "smooth",
            RenderMode::Retro => "retro",
        }
    }

    pub fn cycled(self) -> Self {
        match self {
            RenderMode::PixelPerfect => RenderMode::Smooth,
            RenderMode::Smooth => RenderMode::Retro,
            RenderMode::Retro => RenderMode::PixelPerfect,
        }
    }

    #[cfg(not(target_family = "wasm"))]
    fn parse(value: &str) -> Option<Self> {
        [
            RenderMode::PixelPerfect,
            RenderMode::Smooth,
            RenderMode::Retro,
        ]
        .into_iter()
        .find(|mode| mode.label() == value)
    }
}

const UI_SCALES: [u32; 4] = [75, 100, 125, 150];

fn cycle_ui_scale(scale: u32) -> u32 {
    let index = UI_SCALES
        .iter()
        .position(|&step| step == scale)
        .unwrap_or(1);
    UI_SCALES[(index + 1) % UI_SCALES.len()]
}

#[cfg(not(target_family = "wasm"))]
fn snap_ui_scale(scale: u32) -> u32 {
    UI_SCALES
        .into_iter()
        .min_by_key(|&step| step.abs_diff(scale))
        .unwrap_or(100)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub fullscreen: bool,
    pub vsync: bool,
    pub render_mode: RenderMode,
    pub ui_scale: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            vsync: true,
            render_mode: RenderMode::PixelPerfect,
            ui_scale: 100,
        }
    }
}

impl Settings {
    pub fn cycle_render_mode(&mut self) {
        self.render_mode = self.render_mode.cycled();
    }

    pub fn cycle_ui_scale(&mut self) {
        self.ui_scale = cycle_ui_scale(self.ui_scale);
    }

    pub fn ui_scale_label(&self) -> String {
        format!("UI scale: {}%", self.ui_scale)
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
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match (key.trim(), value.trim()) {
            ("fullscreen", value) => settings.fullscreen = value == "true",
            ("vsync", value) => settings.vsync = value == "true",
            ("render_mode", value) => {
                if let Some(mode) = RenderMode::parse(value) {
                    settings.render_mode = mode;
                }
            }
            ("ui_scale", value) => {
                if let Ok(scale) = value.parse::<u32>() {
                    settings.ui_scale = snap_ui_scale(scale);
                }
            }
            _ => {}
        }
    }
    settings
}

#[cfg(not(target_family = "wasm"))]
pub fn save(settings: &Settings) {
    let _ = std::fs::create_dir_all("saves");
    let content = format!(
        "fullscreen={}\nvsync={}\nrender_mode={}\nui_scale={}\n",
        settings.fullscreen,
        settings.vsync,
        settings.render_mode.label(),
        settings.ui_scale,
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
