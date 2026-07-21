mod items;
mod materials;
mod reactions;

pub use fallingsand_material::Tag;
pub use items::*;
pub use materials::*;
pub use reactions::*;

pub type Color = [u8; 4];

#[derive(Debug, Default)]
pub struct Catalog {
    pub(crate) burning_colors: Vec<Color>,
    pub(crate) materials: Vec<(MaterialKey, MaterialDef)>,
    pub(crate) reactions: Vec<ReactionDef>,
    pub(crate) decays: Vec<DecayDef>,
    pub(crate) items: Vec<(ItemKey, ItemDef)>,
    pub(crate) recipes: Vec<RecipeDef>,
    pub(crate) bonds: Vec<(BondGroup, BondGroup)>,
}

impl Catalog {
    pub fn new(burning_colors: impl IntoIterator<Item = Color>) -> Self {
        Self {
            burning_colors: burning_colors.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn add(&mut self, key: MaterialKey, definition: MaterialDef) {
        self.materials.push((key, definition));
    }

    pub fn react(&mut self, definition: ReactionDef) {
        self.reactions.push(definition);
    }

    pub fn decay(&mut self, from: MaterialKey, into: MaterialKey, rate: f32) {
        self.decays.push(DecayDef { from, into, rate });
    }

    pub fn add_item(&mut self, key: ItemKey, definition: ItemDef) {
        self.items.push((key, definition));
    }

    pub fn craft(&mut self, definition: RecipeDef) {
        self.recipes.push(definition);
    }

    pub fn bond(&mut self, a: BondGroup, b: BondGroup) {
        self.bonds.push((a, b));
    }
}
