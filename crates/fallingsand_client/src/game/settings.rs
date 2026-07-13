use fallingsand_protocol::CursorMode;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
}

#[repr(u16)]
#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiScale {
    #[serde(rename = "75")]
    Percent75 = 75,
    #[default]
    #[serde(rename = "100")]
    Percent100 = 100,
    #[serde(rename = "125")]
    Percent125 = 125,
    #[serde(rename = "150")]
    Percent150 = 150,
}

impl UiScale {
    pub fn percent(self) -> u16 {
        self as u16
    }

    pub fn cycled(self) -> Self {
        match self {
            Self::Percent75 => Self::Percent100,
            Self::Percent100 => Self::Percent125,
            Self::Percent125 => Self::Percent150,
            Self::Percent150 => Self::Percent75,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub fullscreen: bool,
    pub vsync: bool,
    pub render_mode: RenderMode,
    pub ui_scale: UiScale,
    pub cursor_mode: CursorMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            vsync: true,
            render_mode: RenderMode::PixelPerfect,
            ui_scale: UiScale::default(),
            cursor_mode: CursorMode::Smart,
        }
    }
}

impl Settings {
    pub fn cycle_render_mode(&mut self) {
        self.render_mode = self.render_mode.cycled();
    }

    pub fn cycle_ui_scale(&mut self) {
        self.ui_scale = self.ui_scale.cycled();
    }

    pub fn ui_scale_label(&self) -> String {
        format!("UI scale: {}%", self.ui_scale.percent())
    }

    pub fn cycle_cursor_mode(&mut self) {
        self.cursor_mode = self.cursor_mode.cycled();
    }

    pub fn cursor_mode_label(&self) -> String {
        format!("Cursor: {}", self.cursor_mode.label())
    }
}

#[cfg(not(target_family = "wasm"))]
const SETTINGS_PATH: &str = "saves/settings.json";

#[cfg(not(target_family = "wasm"))]
pub fn load() -> Settings {
    let text = match std::fs::read_to_string(SETTINGS_PATH) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(err) => {
            bevy::log::warn!("failed to read settings: {err}");
            return Settings::default();
        }
    };
    serde_json::from_str(&text).unwrap_or_else(|err| {
        bevy::log::warn!("failed to parse settings: {err}");
        Settings::default()
    })
}

#[cfg(not(target_family = "wasm"))]
pub fn save(settings: &Settings) {
    let _ = std::fs::create_dir_all("saves");
    let text = match serde_json::to_string_pretty(settings) {
        Ok(text) => text,
        Err(err) => {
            bevy::log::warn!("failed to serialize settings: {err}");
            return;
        }
    };
    if let Err(err) = std::fs::write(SETTINGS_PATH, text) {
        bevy::log::warn!("failed to persist settings: {err}");
    }
}

#[cfg(target_family = "wasm")]
pub fn load() -> Settings {
    Settings::default()
}

#[cfg(target_family = "wasm")]
pub fn save(_settings: &Settings) {}
