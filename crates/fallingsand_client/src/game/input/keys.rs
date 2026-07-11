use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Key(KeyCode),
    Mouse(MouseButton),
    ScrollUp,
    ScrollDown,
}

impl From<KeyCode> for Button {
    fn from(key: KeyCode) -> Self {
        Button::Key(key)
    }
}

impl From<MouseButton> for Button {
    fn from(button: MouseButton) -> Self {
        Button::Mouse(button)
    }
}

#[derive(Default)]
pub struct RawInput {
    pub pressed: Vec<Button>,
    pub just_pressed: Vec<Button>,
    pub just_released: Vec<Button>,
}

impl RawInput {
    pub fn is_pressed(&self, button: impl Into<Button>) -> bool {
        self.pressed.contains(&button.into())
    }

    pub fn is_just_pressed(&self, button: impl Into<Button>) -> bool {
        self.just_pressed.contains(&button.into())
    }

    pub fn shift(&self) -> bool {
        self.is_pressed(KeyCode::ShiftLeft) || self.is_pressed(KeyCode::ShiftRight)
    }
}
