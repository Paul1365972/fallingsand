use super::MaterialKey;
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemKey(Cow<'static, str>);

impl ItemKey {
    pub const fn new(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ItemKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone)]
pub struct ItemDef {
    pub(crate) display: String,
    pub(crate) stack: u32,
    pub(crate) tool: Option<(u8, f32)>,
}

impl ItemDef {
    pub fn stack(mut self, value: u32) -> Self {
        self.stack = value;
        self
    }

    pub fn tool(mut self, tier: u8, speed: f32) -> Self {
        self.tool = Some((tier, speed));
        self
    }
}

pub fn item(display: impl Into<String>) -> ItemDef {
    ItemDef {
        display: display.into(),
        stack: 1,
        tool: None,
    }
}

#[derive(Debug, Clone)]
pub enum IngredientDef {
    Material(MaterialKey),
    Item(ItemKey),
}

impl From<MaterialKey> for IngredientDef {
    fn from(value: MaterialKey) -> Self {
        Self::Material(value)
    }
}

impl From<ItemKey> for IngredientDef {
    fn from(value: ItemKey) -> Self {
        Self::Item(value)
    }
}

#[derive(Debug, Clone)]
pub struct RecipeDef {
    pub(crate) inputs: Vec<(IngredientDef, u32)>,
    pub(crate) output: (IngredientDef, u32),
}

#[derive(Debug, Clone, Default)]
pub struct RecipeBuilder {
    inputs: Vec<(IngredientDef, u32)>,
}

impl RecipeBuilder {
    pub fn input(mut self, ingredient: impl Into<IngredientDef>, count: u32) -> Self {
        self.inputs.push((ingredient.into(), count));
        self
    }

    pub fn output(self, ingredient: impl Into<IngredientDef>, count: u32) -> RecipeDef {
        RecipeDef {
            inputs: self.inputs,
            output: (ingredient.into(), count),
        }
    }
}

pub fn recipe() -> RecipeBuilder {
    RecipeBuilder::default()
}
