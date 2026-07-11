use super::keys::{Button, RawInput};
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Context {
    Menu,
    Connecting,
    Gameplay,
    Inventory,
    Chat,
    Paused,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    MoveLeft,
    MoveRight,
    Jump,
    Duck,
    Primary,
    Secondary,
    ToggleFlight,
    SelectSlot(u8),
    SlotPrev,
    SlotNext,
    BrushShrink,
    BrushGrow,
    OpenInventory,
    OpenChat,
    Pause,
    CloseOverlay,
    SubmitChat,
    Resume,
    CancelConnect,
    Screenshot,
    ToggleFullscreen,
    ZoomIn,
    ZoomOut,
    ToggleDebugOverlay,
    ToggleDebugBorders,
    CycleGameMode,
    CycleRenderMode,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gesture {
    Press,
    Tap,
    DoubleTap,
    #[expect(dead_code)]
    Hold {
        secs: f32,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modifier {
    Ctrl,
    #[expect(dead_code)]
    Shift,
    #[expect(dead_code)]
    Alt,
    Key(KeyCode),
}

impl Modifier {
    pub fn held(self, raw: &RawInput) -> bool {
        self.buttons()
            .into_iter()
            .any(|button| raw.is_pressed(button))
    }

    pub fn buttons(self) -> [Button; 2] {
        let pair = |a: KeyCode, b: KeyCode| [Button::Key(a), Button::Key(b)];
        match self {
            Modifier::Ctrl => pair(KeyCode::ControlLeft, KeyCode::ControlRight),
            Modifier::Shift => pair(KeyCode::ShiftLeft, KeyCode::ShiftRight),
            Modifier::Alt => pair(KeyCode::AltLeft, KeyCode::AltRight),
            Modifier::Key(key) => pair(key, key),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Binding {
    pub button: Button,
    pub gesture: Gesture,
    pub modifier: Option<Modifier>,
    pub action: Action,
}

pub struct Layer {
    pub opaque: bool,
    pub bindings: Vec<Binding>,
}

pub struct Bindings {
    pub global: Layer,
    pub menu: Layer,
    pub connecting: Layer,
    pub gameplay: Layer,
    pub inventory: Layer,
    pub chat: Layer,
    pub paused: Layer,
}

impl Bindings {
    pub fn layer(&self, context: Context) -> &Layer {
        match context {
            Context::Menu => &self.menu,
            Context::Connecting => &self.connecting,
            Context::Gameplay => &self.gameplay,
            Context::Inventory => &self.inventory,
            Context::Chat => &self.chat,
            Context::Paused => &self.paused,
        }
    }
}

fn bind(button: impl Into<Button>, action: Action) -> Binding {
    Binding {
        button: button.into(),
        gesture: Gesture::Press,
        modifier: None,
        action,
    }
}

fn chord(modifier: Modifier, button: impl Into<Button>, action: Action) -> Binding {
    Binding {
        button: button.into(),
        gesture: Gesture::Press,
        modifier: Some(modifier),
        action,
    }
}

fn tap(button: impl Into<Button>, action: Action) -> Binding {
    Binding {
        button: button.into(),
        gesture: Gesture::Tap,
        modifier: None,
        action,
    }
}

fn double_tap(button: impl Into<Button>, action: Action) -> Binding {
    Binding {
        button: button.into(),
        gesture: Gesture::DoubleTap,
        modifier: None,
        action,
    }
}

fn opaque(bindings: Vec<Binding>) -> Layer {
    Layer {
        opaque: true,
        bindings,
    }
}

fn zoom_chords() -> [Binding; 2] {
    [
        chord(Modifier::Ctrl, Button::ScrollUp, Action::ZoomIn),
        chord(Modifier::Ctrl, Button::ScrollDown, Action::ZoomOut),
    ]
}

const SLOT_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

impl Default for Bindings {
    fn default() -> Self {
        let mut gameplay = vec![
            bind(KeyCode::KeyA, Action::MoveLeft),
            bind(KeyCode::ArrowLeft, Action::MoveLeft),
            bind(KeyCode::KeyD, Action::MoveRight),
            bind(KeyCode::ArrowRight, Action::MoveRight),
            bind(KeyCode::Space, Action::Jump),
            bind(KeyCode::KeyW, Action::Jump),
            bind(KeyCode::ArrowUp, Action::Jump),
            bind(KeyCode::KeyS, Action::Duck),
            bind(KeyCode::ArrowDown, Action::Duck),
            bind(MouseButton::Left, Action::Primary),
            bind(MouseButton::Right, Action::Secondary),
            double_tap(KeyCode::Space, Action::ToggleFlight),
            bind(Button::ScrollUp, Action::SlotPrev),
            bind(Button::ScrollDown, Action::SlotNext),
            bind(KeyCode::BracketLeft, Action::BrushShrink),
            bind(KeyCode::Minus, Action::BrushShrink),
            bind(KeyCode::BracketRight, Action::BrushGrow),
            bind(KeyCode::Equal, Action::BrushGrow),
            bind(KeyCode::KeyE, Action::OpenInventory),
            bind(KeyCode::Enter, Action::OpenChat),
            bind(KeyCode::Escape, Action::Pause),
        ];
        for (index, key) in SLOT_KEYS.into_iter().enumerate() {
            gameplay.push(bind(key, Action::SelectSlot(index as u8)));
        }
        gameplay.extend(zoom_chords());

        let mut inventory = vec![
            bind(KeyCode::KeyE, Action::CloseOverlay),
            bind(KeyCode::Escape, Action::CloseOverlay),
        ];
        inventory.extend(zoom_chords());

        let mut chat = vec![
            bind(KeyCode::Enter, Action::SubmitChat),
            bind(KeyCode::Escape, Action::CloseOverlay),
        ];
        chat.extend(zoom_chords());

        Self {
            global: opaque(vec![
                bind(KeyCode::F2, Action::Screenshot),
                bind(KeyCode::F11, Action::ToggleFullscreen),
                chord(
                    Modifier::Key(KeyCode::F3),
                    KeyCode::KeyG,
                    Action::ToggleDebugBorders,
                ),
                chord(
                    Modifier::Key(KeyCode::F3),
                    KeyCode::KeyN,
                    Action::CycleGameMode,
                ),
                chord(
                    Modifier::Key(KeyCode::F3),
                    KeyCode::KeyR,
                    Action::CycleRenderMode,
                ),
                tap(KeyCode::F3, Action::ToggleDebugOverlay),
            ]),
            menu: opaque(Vec::new()),
            connecting: opaque(vec![bind(KeyCode::Escape, Action::CancelConnect)]),
            gameplay: opaque(gameplay),
            inventory: opaque(inventory),
            chat: opaque(chat),
            paused: opaque(vec![bind(KeyCode::Escape, Action::Resume)]),
        }
    }
}
