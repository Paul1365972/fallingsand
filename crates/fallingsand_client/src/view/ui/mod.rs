pub mod chat;
pub mod connscreen;
pub mod debug;
pub mod hud;
pub mod inventory;
pub mod menu;
pub mod pause;

use super::io::Btn;
use bevy::prelude::*;
use bevy::text::TextCursorStyle;
use fallingsand_core::{IconSpec, ItemId, ItemRegistry};

pub const BUTTON_BG: Color = Color::srgb(0.14, 0.16, 0.22);
pub const BUTTON_HOVER: Color = Color::srgb(0.22, 0.25, 0.33);

pub fn item_color(item_reg: &ItemRegistry, item: ItemId) -> [u8; 4] {
    match item_reg.try_get(item).map(|def| def.icon) {
        Some(IconSpec::MaterialSwatch(material)) => {
            fallingsand_core::content::material(material).colors[0]
        }
        _ => [180, 180, 190, 255],
    }
}

pub fn format_count(count: u32) -> String {
    if count >= 100_000 {
        format!("{}k", count / 1000)
    } else {
        format!("{count}")
    }
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
    parent
        .spawn((
            action,
            Button,
            ButtonBase(background),
            Node {
                width: px(width),
                height: px(30),
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
