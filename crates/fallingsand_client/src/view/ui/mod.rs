pub mod chat;
pub mod connscreen;
pub mod debug;
pub mod game_menu;
pub mod hud;
pub mod icons;
pub mod inventory;
pub mod menu;
pub mod settings;
pub(crate) mod slots;

use super::io::Btn;
use bevy::prelude::*;
use bevy::text::TextCursorStyle;

pub const BUTTON_BG: Color = Color::srgb(0.14, 0.16, 0.22);
pub const BUTTON_HOVER: Color = Color::srgb(0.22, 0.25, 0.33);

pub mod depth {
    pub const HUD_LABEL: i32 = 1;
    pub const CHAT: i32 = 20;
    pub const INVENTORY: i32 = 30;
    pub const INVENTORY_CURSOR: i32 = 40;
    pub const INVENTORY_TOOLTIP: i32 = 41;
    pub const DAMAGE_FLASH: i32 = 50;
    pub const DEATH: i32 = 60;
    pub const CONNECTION: i32 = 65;
    pub const GAME_MENU: i32 = 70;
    pub const SETTINGS: i32 = 80;
    pub const DEBUG: i32 = 100;
}

pub fn field_cursor_style() -> TextCursorStyle {
    TextCursorStyle {
        color: Color::srgb(0.9, 0.8, 0.5),
        selection_color: Color::srgba(0.3, 0.45, 0.75, 0.7),
        unfocused_selection_color: Color::srgba(0.3, 0.45, 0.75, 0.35),
        selected_text_color: None,
    }
}

#[derive(Component)]
pub struct ButtonBase(pub Color);

pub fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    action: Btn,
    label: &str,
    width: f32,
    background: Color,
) {
    spawn_button_with(parent, (), action, label, width, background, Display::Flex);
}

pub fn spawn_button_with(
    parent: &mut ChildSpawnerCommands,
    marker: impl Bundle,
    action: Btn,
    label: &str,
    width: f32,
    background: Color,
    display: Display,
) {
    parent
        .spawn((
            marker,
            action,
            Button,
            ButtonBase(background),
            Node {
                width: px(width),
                height: px(30),
                display,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(background),
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(15.0),
                ..default()
            },
            TextColor(Color::WHITE),
        ));
}

pub fn button_hover(
    mut buttons: Query<(&Interaction, &ButtonBase, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, base, mut background) in &mut buttons {
        background.0 = match interaction {
            Interaction::None => base.0,
            Interaction::Hovered | Interaction::Pressed => BUTTON_HOVER,
        };
    }
}

pub fn set_text(text: &mut Text, value: String) {
    if **text != value {
        **text = value;
    }
}
