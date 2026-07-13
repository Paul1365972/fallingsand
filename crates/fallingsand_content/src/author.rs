use std::borrow::Cow;

pub use fallingsand_material::Tag;

pub type Color = [u8; 4];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MaterialKey(Cow<'static, str>);

impl MaterialKey {
    pub const fn new(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    pub fn owned(name: impl Into<String>) -> Self {
        Self(Cow::Owned(name.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MaterialKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[macro_export]
macro_rules! material_keys {
    ($($name:ident),* $(,)?) => {
        $(pub const $name: $crate::MaterialKey = $crate::MaterialKey::new(stringify!($name));)*
    };
}

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

#[macro_export]
macro_rules! item_keys {
    ($($name:ident),* $(,)?) => {
        $(pub const $name: $crate::ItemKey = $crate::ItemKey::new(stringify!($name));)*
    };
}

#[derive(Debug, Clone, Copy)]
pub enum PhaseDef {
    Empty,
    Solid(SolidDef),
    Powder(PowderDef),
    Liquid(LiquidDef),
    Gas(GasDef),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SolidDef {
    pub(crate) rigid_capable: bool,
}

impl SolidDef {
    pub fn rigid(mut self) -> Self {
        self.rigid_capable = true;
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PowderDef {
    pub(crate) drag: f32,
    pub(crate) friction: f32,
    pub(crate) repose: f32,
    pub(crate) redirect_keep: f32,
    pub(crate) cohesion: f32,
}

impl Default for PowderDef {
    fn default() -> Self {
        Self {
            drag: 0.0,
            friction: 0.0,
            repose: 0.0,
            redirect_keep: 1.0,
            cohesion: 0.0,
        }
    }
}

impl PowderDef {
    pub fn drag(mut self, value: f32) -> Self {
        self.drag = value;
        self
    }

    pub fn friction(mut self, value: f32) -> Self {
        self.friction = value;
        self
    }

    pub fn repose(mut self, value: f32) -> Self {
        self.repose = value;
        self
    }

    pub fn redirect_keep(mut self, value: f32) -> Self {
        self.redirect_keep = value;
        self
    }

    pub fn cohesion(mut self, value: f32) -> Self {
        self.cohesion = value;
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LiquidDef {
    pub(crate) drag: f32,
    pub(crate) friction: f32,
    pub(crate) redirect_keep: f32,
    pub(crate) cohesion: f32,
    pub(crate) flow_rate: f32,
}

impl Default for LiquidDef {
    fn default() -> Self {
        Self {
            drag: 0.0,
            friction: 0.0,
            redirect_keep: 1.0,
            cohesion: 0.0,
            flow_rate: 0.0,
        }
    }
}

impl LiquidDef {
    pub fn drag(mut self, value: f32) -> Self {
        self.drag = value;
        self
    }

    pub fn friction(mut self, value: f32) -> Self {
        self.friction = value;
        self
    }

    pub fn redirect_keep(mut self, value: f32) -> Self {
        self.redirect_keep = value;
        self
    }

    pub fn cohesion(mut self, value: f32) -> Self {
        self.cohesion = value;
        self
    }

    pub fn flow_rate(mut self, value: f32) -> Self {
        self.flow_rate = value;
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GasDef {
    pub(crate) drag: f32,
    pub(crate) cohesion: f32,
    pub(crate) turbulence: f32,
    pub(crate) redirect_keep: f32,
}

impl Default for GasDef {
    fn default() -> Self {
        Self {
            drag: 0.0,
            cohesion: 0.0,
            turbulence: 0.0,
            redirect_keep: 1.0,
        }
    }
}

impl GasDef {
    pub fn drag(mut self, value: f32) -> Self {
        self.drag = value;
        self
    }

    pub fn cohesion(mut self, value: f32) -> Self {
        self.cohesion = value;
        self
    }

    pub fn turbulence(mut self, value: f32) -> Self {
        self.turbulence = value;
        self
    }

    pub fn redirect_keep(mut self, value: f32) -> Self {
        self.redirect_keep = value;
        self
    }
}

impl From<SolidDef> for PhaseDef {
    fn from(value: SolidDef) -> Self {
        Self::Solid(value)
    }
}

impl From<PowderDef> for PhaseDef {
    fn from(value: PowderDef) -> Self {
        Self::Powder(value)
    }
}

impl From<LiquidDef> for PhaseDef {
    fn from(value: LiquidDef) -> Self {
        Self::Liquid(value)
    }
}

impl From<GasDef> for PhaseDef {
    fn from(value: GasDef) -> Self {
        Self::Gas(value)
    }
}

pub fn empty() -> PhaseDef {
    PhaseDef::Empty
}

pub fn solid() -> SolidDef {
    SolidDef::default()
}

pub fn powder() -> PowderDef {
    PowderDef::default()
}

pub fn liquid() -> LiquidDef {
    LiquidDef::default()
}

pub fn gas() -> GasDef {
    GasDef::default()
}

#[derive(Debug, Clone, Default)]
pub struct BurningDef {
    pub(crate) ignite: f32,
    pub(crate) smoulder: f32,
    pub(crate) rate: f32,
    pub(crate) emit: f32,
    pub(crate) colors: Vec<Color>,
    pub(crate) residue: Option<MaterialKey>,
    pub(crate) residue_chance: f32,
    pub(crate) burnout: Option<MaterialKey>,
    pub(crate) damage: f32,
}

impl BurningDef {
    pub fn ignite(mut self, value: f32) -> Self {
        self.ignite = value;
        self
    }

    pub fn smoulder(mut self, value: f32) -> Self {
        self.smoulder = value;
        self
    }

    pub fn rate(mut self, value: f32) -> Self {
        self.rate = value;
        self
    }

    pub fn emit(mut self, value: f32) -> Self {
        self.emit = value;
        self
    }

    pub fn colors(mut self, value: impl IntoIterator<Item = Color>) -> Self {
        self.colors = value.into_iter().collect();
        self
    }

    pub fn residue(mut self, material: MaterialKey, chance: f32) -> Self {
        self.residue = Some(material);
        self.residue_chance = chance;
        self
    }

    pub fn burnout(mut self, material: MaterialKey) -> Self {
        self.burnout = Some(material);
        self
    }

    pub fn damage(mut self, value: f32) -> Self {
        self.damage = value;
        self
    }
}

pub fn burning() -> BurningDef {
    BurningDef::default()
}

#[derive(Debug, Clone, Default)]
pub struct EmberDef {
    pub(crate) rate: f32,
    pub(crate) emit: f32,
    pub(crate) residue: Option<MaterialKey>,
    pub(crate) residue_chance: f32,
    pub(crate) burnout: Option<MaterialKey>,
}

impl EmberDef {
    pub fn rate(mut self, value: f32) -> Self {
        self.rate = value;
        self
    }

    pub fn emit(mut self, value: f32) -> Self {
        self.emit = value;
        self
    }

    pub fn residue(mut self, material: MaterialKey, chance: f32) -> Self {
        self.residue = Some(material);
        self.residue_chance = chance;
        self
    }

    pub fn burnout(mut self, material: MaterialKey) -> Self {
        self.burnout = Some(material);
        self
    }
}

pub fn ember() -> EmberDef {
    EmberDef::default()
}

#[derive(Debug, Clone, Default)]
pub struct MaterialDef {
    pub(crate) base: Option<MaterialKey>,
    pub(crate) phase: Option<PhaseDef>,
    pub(crate) density: Option<f32>,
    pub(crate) colors: Option<Vec<Color>>,
    pub(crate) surface_grip: Option<f32>,
    pub(crate) hardness: Option<f32>,
    pub(crate) restitution: Option<f32>,
    pub(crate) surface_bounce: Option<f32>,
    pub(crate) contact_damage: Option<f32>,
    pub(crate) tags: Option<Vec<Tag>>,
    pub(crate) burn: Option<BurningDef>,
    pub(crate) ember: Option<EmberDef>,
}

impl MaterialDef {
    pub fn phase(mut self, value: impl Into<PhaseDef>) -> Self {
        self.phase = Some(value.into());
        self
    }

    pub fn density(mut self, value: f32) -> Self {
        self.density = Some(value);
        self
    }

    pub fn colors(mut self, value: impl IntoIterator<Item = Color>) -> Self {
        self.colors = Some(value.into_iter().collect());
        self
    }

    pub fn surface_grip(mut self, value: f32) -> Self {
        self.surface_grip = Some(value);
        self
    }

    pub fn hardness(mut self, value: f32) -> Self {
        self.hardness = Some(value);
        self
    }

    pub fn restitution(mut self, value: f32) -> Self {
        self.restitution = Some(value);
        self
    }

    pub fn surface_bounce(mut self, value: f32) -> Self {
        self.surface_bounce = Some(value);
        self
    }

    pub fn contact_damage(mut self, value: f32) -> Self {
        self.contact_damage = Some(value);
        self
    }

    pub fn tag(mut self, tag: Tag) -> Self {
        self.tags.get_or_insert_default().push(tag);
        self
    }

    pub fn tags(mut self, tags: impl IntoIterator<Item = Tag>) -> Self {
        self.tags = Some(tags.into_iter().collect());
        self
    }

    pub fn burning(mut self, value: BurningDef) -> Self {
        self.burn = Some(value);
        self
    }

    pub fn ember(mut self, value: EmberDef) -> Self {
        self.ember = Some(value);
        self
    }
}

pub fn material(phase: impl Into<PhaseDef>) -> MaterialDef {
    MaterialDef::default().phase(phase)
}

pub fn inherit(base: MaterialKey) -> MaterialDef {
    MaterialDef {
        base: Some(base),
        ..MaterialDef::default()
    }
}

#[derive(Debug, Clone)]
pub enum OperandDef {
    Material(MaterialKey),
    Tag(Tag),
}

impl From<MaterialKey> for OperandDef {
    fn from(value: MaterialKey) -> Self {
        Self::Material(value)
    }
}

pub fn tagged(tag: Tag) -> OperandDef {
    OperandDef::Tag(tag)
}

#[derive(Debug, Clone)]
pub enum ProductDef {
    Material(MaterialKey),
    Same(Tag),
}

impl From<MaterialKey> for ProductDef {
    fn from(value: MaterialKey) -> Self {
        Self::Material(value)
    }
}

pub fn same(tag: Tag) -> ProductDef {
    ProductDef::Same(tag)
}

#[derive(Debug, Clone)]
pub struct ReactionDef {
    pub(crate) a: OperandDef,
    pub(crate) b: OperandDef,
    pub(crate) a_becomes: ProductDef,
    pub(crate) b_becomes: ProductDef,
    pub(crate) rate: f32,
}

pub struct ReactionBuilder {
    a: OperandDef,
    b: OperandDef,
}

impl ReactionBuilder {
    pub fn becomes(
        self,
        a: impl Into<ProductDef>,
        b: impl Into<ProductDef>,
    ) -> ReactionProductsBuilder {
        ReactionProductsBuilder {
            a: self.a,
            b: self.b,
            a_becomes: a.into(),
            b_becomes: b.into(),
        }
    }
}

pub struct ReactionProductsBuilder {
    a: OperandDef,
    b: OperandDef,
    a_becomes: ProductDef,
    b_becomes: ProductDef,
}

impl ReactionProductsBuilder {
    pub fn rate(self, rate: f32) -> ReactionDef {
        ReactionDef {
            a: self.a,
            b: self.b,
            a_becomes: self.a_becomes,
            b_becomes: self.b_becomes,
            rate,
        }
    }
}

pub fn reaction(a: impl Into<OperandDef>, b: impl Into<OperandDef>) -> ReactionBuilder {
    ReactionBuilder {
        a: a.into(),
        b: b.into(),
    }
}

#[derive(Debug, Clone)]
pub struct DecayDef {
    pub(crate) from: MaterialKey,
    pub(crate) into: MaterialKey,
    pub(crate) rate: f32,
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

#[derive(Debug, Clone)]
pub struct ThresholdDef {
    pub(crate) name: String,
    pub(crate) rate: f32,
}

#[derive(Debug, Default)]
pub struct Catalog {
    pub(crate) ember_colors: Vec<Color>,
    pub(crate) materials: Vec<(MaterialKey, MaterialDef)>,
    pub(crate) reactions: Vec<ReactionDef>,
    pub(crate) decays: Vec<DecayDef>,
    pub(crate) items: Vec<(ItemKey, ItemDef)>,
    pub(crate) recipes: Vec<RecipeDef>,
    pub(crate) thresholds: Vec<ThresholdDef>,
}

impl Catalog {
    pub fn new(ember_colors: impl IntoIterator<Item = Color>) -> Self {
        Self {
            ember_colors: ember_colors.into_iter().collect(),
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

    pub fn threshold(&mut self, name: impl Into<String>, rate: f32) {
        self.thresholds.push(ThresholdDef {
            name: name.into(),
            rate,
        });
    }
}
