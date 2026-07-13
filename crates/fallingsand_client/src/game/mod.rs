pub mod chat;
pub mod clock;
pub mod debug;
pub mod identity;
pub mod input;
pub mod inventory;
pub mod menu;
pub mod net;
pub mod players;
pub mod settings;
pub mod world;

use bevy::math::Vec2;
use chat::Chat;
use clock::WorldClock;
use debug::DebugState;
use fallingsand_core::{CellPos, ItemRegistry, RecipeRegistry};
use input::{Bindings, InputCore, RawInput};
use inventory::{Inventory, SlotRegion};
use menu::MenuState;
use net::{ConnectTarget, Net};
use players::{Players, SelfState};
use settings::Settings;
use std::sync::Arc;
use world::WorldView;

pub use settings::RenderMode;

pub struct Registries {
    pub items: Arc<ItemRegistry>,
    pub recipes: Arc<RecipeRegistry>,
}

pub struct ViewPrefs {
    pub zoom_index: i32,
    pub debug_overlay: bool,
    pub debug_borders: bool,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            zoom_index: 0,
            debug_overlay: true,
            debug_borders: false,
        }
    }
}

#[derive(Default)]
pub struct Changes {
    pub slots: Vec<usize>,
    pub trash: bool,
    pub chat: bool,
    pub chat_draft: bool,
    pub roster: bool,
    pub mode: bool,
    pub worlds: bool,
    pub settings: bool,
}

impl Changes {
    fn clear(&mut self) {
        *self = Self::default();
    }
}

pub enum Effect {
    Screenshot,
    ApplyWindow,
    Quit,
}

pub enum UiEvent {
    NameEdited(String),
    Play(String),
    CreateWorld(String),
    DeleteWorld(String),
    Connect { url: String, cert_hex: String },
    ToggleFullscreen,
    ToggleVsync,
    CycleRenderMode,
    CycleUiScale,
    CycleCursorMode,
    OpenSettings,
    CloseSettings,
    QuitApp,
    PauseResume,
    PauseSave,
    PauseQuitToMenu,
    CancelConnect,
    Revive,
    Slot { region: SlotRegion, right: bool },
}

pub struct IoFrame {
    pub dt: f32,
    pub now: f32,
    pub raw: RawInput,
    pub zoom_base: u32,
    pub cursor_cell: Option<CellPos>,
    pub over_ui: bool,
    pub chat_text: Option<String>,
    pub ui_events: Vec<UiEvent>,
}

pub enum Flow {
    Menu,
    InGame(Box<InGame>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Connecting,
    Playing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    Inventory,
    Chat,
    Paused,
}

pub struct InGame {
    pub net: Net,
    pub phase: Phase,
    overlays: Vec<Overlay>,
    pub world: WorldView,
    pub players: Players,
    pub you: SelfState,
    pub inventory: Inventory,
    pub chat: Chat,
    pub clock: WorldClock,
    pub debug: DebugState,
}

impl InGame {
    fn new(net: Net) -> Self {
        Self {
            net,
            phase: Phase::Connecting,
            overlays: Vec::new(),
            world: WorldView::default(),
            players: Players::default(),
            you: SelfState::default(),
            inventory: Inventory::default(),
            chat: Chat::default(),
            clock: WorldClock::default(),
            debug: DebugState::default(),
        }
    }

    pub fn overlays(&self) -> &[Overlay] {
        &self.overlays
    }

    pub fn overlay_top(&self) -> Option<Overlay> {
        self.overlays.last().copied()
    }

    pub fn paused(&self) -> bool {
        self.overlays.contains(&Overlay::Paused)
    }

    pub fn chat_open(&self) -> bool {
        self.overlays.contains(&Overlay::Chat)
    }

    pub fn inventory_open(&self) -> bool {
        self.overlays.contains(&Overlay::Inventory)
    }

    pub fn open_overlay(&mut self, overlay: Overlay, input: &mut InputCore) {
        if self.overlays.contains(&overlay) {
            return;
        }
        self.overlays.push(overlay);
        if overlay == Overlay::Chat {
            self.chat.begin_history();
        }
        if overlay == Overlay::Paused {
            self.net.set_embedded_paused(true);
            input.release_held(self.net.session.as_mut());
        }
    }

    pub fn close_overlay(&mut self, overlay: Overlay) {
        if !self.overlays.contains(&overlay) {
            return;
        }
        self.overlays.retain(|&open| open != overlay);
        if overlay == Overlay::Paused {
            self.net.set_embedded_paused(false);
        }
    }

    fn on_session_lost(&mut self, changes: &mut Changes, input: &mut InputCore) {
        self.world.clear();
        self.players.clear();
        self.you = SelfState::default();
        self.inventory.reset(changes);
        self.overlays.retain(|&open| open != Overlay::Inventory);
        self.debug.rects.clear();
        self.debug.subscribed = false;
        changes.roster = true;
        changes.mode = true;
        input.reset();
    }
}

pub struct ClientGame {
    pub registries: Registries,
    pub settings: Settings,
    pub menu: MenuState,
    pub flow: Flow,
    pub bindings: Bindings,
    pub input: InputCore,
    pub view_prefs: ViewPrefs,
    pub settings_open: bool,
    pub changes: Changes,
    pub effects: Vec<Effect>,
}

impl ClientGame {
    pub fn new(registries: Registries) -> Self {
        Self {
            registries,
            settings: settings::load(),
            menu: MenuState::scan(),
            flow: Flow::Menu,
            bindings: Bindings::default(),
            input: InputCore::default(),
            view_prefs: ViewPrefs::default(),
            settings_open: false,
            changes: Changes::default(),
            effects: vec![Effect::ApplyWindow],
        }
    }

    pub fn ingame(&self) -> Option<&InGame> {
        match &self.flow {
            Flow::InGame(ingame) => Some(ingame),
            Flow::Menu => None,
        }
    }

    pub fn ingame_mut(&mut self) -> Option<&mut InGame> {
        match &mut self.flow {
            Flow::InGame(ingame) => Some(ingame),
            Flow::Menu => None,
        }
    }

    pub fn playing(&self) -> Option<&InGame> {
        self.ingame()
            .filter(|ingame| ingame.phase == Phase::Playing)
    }

    pub fn update(&mut self, io: &mut IoFrame) {
        self.changes.clear();
        for event in std::mem::take(&mut io.ui_events) {
            self.apply_ui_event(event, io);
        }
        input::resolve(self, io);
        net::update(self, io);
        input::flush(self, io.dt);
        self.tick_timers(io.dt);
    }

    fn tick_timers(&mut self, dt: f32) {
        if let Some(ingame) = self.ingame_mut() {
            ingame.you.damage_flash = (ingame.you.damage_flash - dt).max(0.0);
        }
    }

    fn apply_ui_event(&mut self, event: UiEvent, io: &IoFrame) {
        match event {
            UiEvent::NameEdited(name) => {
                identity::update_name(&name);
            }
            UiEvent::Play(world) => self.start_game_local(world),
            UiEvent::CreateWorld(raw) => {
                if let Some(name) = menu::sanitize_world_name(&raw) {
                    self.start_game_local(name);
                }
            }
            UiEvent::DeleteWorld(name) => {
                self.menu.confirm_delete(name);
                self.changes.worlds = true;
            }
            UiEvent::Connect { url, cert_hex } => {
                let url = url.trim().to_string();
                if !url.is_empty() {
                    self.start_game_remote(ConnectTarget {
                        url,
                        cert_hash: net::parse_cert_hash(cert_hex.trim()),
                    });
                }
            }
            UiEvent::ToggleFullscreen => self.toggle_fullscreen(),
            UiEvent::ToggleVsync => {
                self.settings.vsync = !self.settings.vsync;
                self.apply_settings();
            }
            UiEvent::CycleRenderMode => {
                self.settings.cycle_render_mode();
                self.apply_settings();
            }
            UiEvent::CycleUiScale => {
                self.settings.cycle_ui_scale();
                self.apply_settings();
            }
            UiEvent::CycleCursorMode => {
                self.settings.cycle_cursor_mode();
                settings::save(&self.settings);
                self.changes.settings = true;
            }
            UiEvent::OpenSettings => self.settings_open = true,
            UiEvent::CloseSettings => self.settings_open = false,
            UiEvent::QuitApp => self.effects.push(Effect::Quit),
            UiEvent::PauseResume => {
                if let Flow::InGame(ingame) = &mut self.flow {
                    ingame.close_overlay(Overlay::Paused);
                }
            }
            UiEvent::PauseSave => {
                if let Some(ingame) = self.ingame() {
                    ingame.net.request_embedded_save();
                }
            }
            UiEvent::PauseQuitToMenu | UiEvent::CancelConnect => self.leave_game(),
            UiEvent::Revive => self.input.queue(fallingsand_protocol::InputAction::Revive),
            UiEvent::Slot { region, right } => {
                let shift = io.raw.shift();
                if let Flow::InGame(ingame) = &mut self.flow
                    && ingame.inventory_open()
                    && let Some(action) = inventory::slot_action(region, right, shift)
                {
                    self.input
                        .queue(fallingsand_protocol::InputAction::Slot(action));
                }
            }
        }
    }

    pub fn toggle_fullscreen(&mut self) {
        self.settings.fullscreen = !self.settings.fullscreen;
        self.apply_settings();
    }

    fn apply_settings(&mut self) {
        settings::save(&self.settings);
        self.changes.settings = true;
        self.effects.push(Effect::ApplyWindow);
    }

    pub fn start_game_local(&mut self, world: String) {
        #[cfg(not(target_family = "wasm"))]
        {
            let net = Net::embedded(world);
            self.enter_game(net);
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = world;
            bevy::log::warn!("no server configured; use the direct-connect menu");
        }
    }

    fn start_game_remote(&mut self, target: ConnectTarget) {
        self.enter_game(Net::remote(target));
    }

    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    fn enter_game(&mut self, net: Net) {
        self.input.reset();
        self.settings_open = false;
        self.flow = Flow::InGame(Box::new(InGame::new(net)));
    }

    pub fn leave_game(&mut self) {
        if let Flow::InGame(ingame) = &mut self.flow
            && let Some(session) = ingame.net.session.as_mut()
        {
            session.send(&fallingsand_protocol::ClientMessage::Goodbye);
        }
        self.flow = Flow::Menu;
        self.menu.rescan();
        self.changes.worlds = true;
        self.input.reset();
        self.settings_open = false;
        self.view_prefs.zoom_index = 0;
    }

    pub(crate) fn player_pos(&self) -> Option<Vec2> {
        self.ingame()
            .filter(|ingame| ingame.you.present)
            .map(|ingame| ingame.you.pos)
    }
}
