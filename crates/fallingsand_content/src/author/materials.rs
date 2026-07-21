use super::{Color, Tag};
use std::borrow::Cow;

const DEFAULT_SEALED_BURN: f32 = 0.1;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MaterialKey(Cow<'static, str>);

impl MaterialKey {
    pub const fn new(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
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

#[derive(Debug, Clone, Copy)]
pub enum PhaseDef {
    Empty,
    Solid(SolidDef),
    Powder(PowderDef),
    Liquid(LiquidDef),
    Gas(GasDef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondGroup {
    Mineral,
    Wood,
    Foliage,
    Ice,
}

pub const BOND_GROUP_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, Default)]
pub struct SolidDef {
    pub(crate) bond: Option<BondGroup>,
}

impl SolidDef {
    pub fn rigid(mut self, group: BondGroup) -> Self {
        self.bond = Some(group);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PowderDef {
    pub(crate) air_drag: f32,
    pub(crate) ground_friction: f32,
    pub(crate) topple_start: f32,
    pub(crate) topple_keep: f32,
    pub(crate) deflect: f32,
}

impl Default for PowderDef {
    fn default() -> Self {
        Self {
            air_drag: 0.0,
            ground_friction: 0.0,
            topple_start: 0.0,
            topple_keep: 0.0,
            deflect: 1.0,
        }
    }
}

impl PowderDef {
    pub fn air_drag(mut self, value: f32) -> Self {
        self.air_drag = value;
        self
    }

    pub fn ground_friction(mut self, value: f32) -> Self {
        self.ground_friction = value;
        self
    }

    pub fn topple(mut self, start: f32, keep: f32) -> Self {
        self.topple_start = start;
        self.topple_keep = keep;
        self
    }

    pub fn deflect(mut self, value: f32) -> Self {
        self.deflect = value;
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LiquidDef {
    pub(crate) drag: f32,
    pub(crate) impact: f32,
    pub(crate) flow_rate: Option<f32>,
}

impl Default for LiquidDef {
    fn default() -> Self {
        Self {
            drag: 0.0,
            impact: 1.0,
            flow_rate: None,
        }
    }
}

impl LiquidDef {
    pub fn drag(mut self, value: f32) -> Self {
        self.drag = value;
        self
    }

    pub fn impact(mut self, value: f32) -> Self {
        self.impact = value;
        self
    }

    pub fn flow_rate(mut self, value: f32) -> Self {
        self.flow_rate = Some(value);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GasDef {
    pub(crate) air_drag: f32,
    pub(crate) turbulence: f32,
    pub(crate) flow_rate: Option<f32>,
}

impl Default for GasDef {
    fn default() -> Self {
        Self {
            air_drag: 0.0,
            turbulence: 0.0,
            flow_rate: None,
        }
    }
}

impl GasDef {
    pub fn air_drag(mut self, value: f32) -> Self {
        self.air_drag = value;
        self
    }

    pub fn turbulence(mut self, value: f32) -> Self {
        self.turbulence = value;
        self
    }

    pub fn flow_rate(mut self, value: f32) -> Self {
        self.flow_rate = Some(value);
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
pub struct FlammableDef {
    pub(crate) ignite: f32,
    pub(crate) sealed_burn: f32,
    pub(crate) rate: f32,
    pub(crate) emit: f32,
    pub(crate) colors: Vec<Color>,
    pub(crate) residue: Option<MaterialKey>,
    pub(crate) residue_chance: f32,
    pub(crate) burnout: Option<MaterialKey>,
    pub(crate) damage: f32,
}

impl FlammableDef {
    pub fn ignite(mut self, value: f32) -> Self {
        self.ignite = value;
        self
    }

    pub fn sealed_burn(mut self, value: f32) -> Self {
        self.sealed_burn = value;
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

pub fn flammable() -> FlammableDef {
    FlammableDef {
        sealed_burn: DEFAULT_SEALED_BURN,
        ..FlammableDef::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct BurningDef {
    pub(crate) rate: f32,
    pub(crate) sealed_burn: f32,
    pub(crate) emit: f32,
    pub(crate) residue: Option<MaterialKey>,
    pub(crate) residue_chance: f32,
    pub(crate) burnout: Option<MaterialKey>,
    pub(crate) base: Option<fallingsand_material::MaterialId>,
}

impl BurningDef {
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

pub fn burning() -> BurningDef {
    BurningDef::default()
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EmissionDef {
    pub(crate) color: [u8; 3],
    pub(crate) intensity: f32,
    pub(crate) flicker: f32,
}

impl EmissionDef {
    pub fn intensity(mut self, value: f32) -> Self {
        self.intensity = value;
        self
    }

    pub fn flicker(mut self, value: f32) -> Self {
        self.flicker = value;
        self
    }
}

pub fn emission(color: [u8; 3]) -> EmissionDef {
    EmissionDef {
        color,
        intensity: 1.0,
        flicker: 0.0,
    }
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
    pub(crate) flammable: Option<FlammableDef>,
    pub(crate) burning: Option<BurningDef>,
    pub(crate) emission: Option<EmissionDef>,
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

    pub fn flammable(mut self, value: FlammableDef) -> Self {
        self.flammable = Some(value);
        self
    }

    pub fn burning(mut self, value: BurningDef) -> Self {
        self.burning = Some(value);
        self
    }

    pub fn emission(mut self, value: EmissionDef) -> Self {
        self.emission = Some(value);
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
